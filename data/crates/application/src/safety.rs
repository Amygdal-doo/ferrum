//! Pre-execution warnings for risky statements.
//!
//! This is a UX guard only, never a security boundary: SQL is too rich for
//! keyword checks to be reliable. Real protection comes from read-only
//! sessions and database roles.

use ferrumdata_domain::{ConnectionProfile, DatabaseEngine};

const MUTATING: &[&str] = &[
    "INSERT", "UPDATE", "DELETE", "MERGE", "UPSERT", "DROP", "TRUNCATE", "ALTER", "CREATE",
    "GRANT", "REVOKE", "COPY", "CALL", "DO", "VACUUM", "REINDEX", "CLUSTER", "COMMENT", "REFRESH",
];

/// First keyword after leading comments/whitespace/parentheses, upper-cased.
#[must_use]
pub fn leading_keyword(sql: &str) -> Option<String> {
    let mut s = sql;
    loop {
        s = s.trim_start_matches(|c: char| c.is_whitespace() || c == '(');
        if let Some(rest) = s.strip_prefix("--") {
            s = rest.split_once('\n').map_or("", |(_, r)| r);
        } else if let Some(rest) = s.strip_prefix("/*") {
            s = rest.split_once("*/").map_or("", |(_, r)| r);
        } else {
            break;
        }
    }
    let word: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    (!word.is_empty()).then(|| word.to_ascii_uppercase())
}

/// True if the statement may modify data or schema. `WITH` is treated as
/// possibly mutating because CTEs can contain DML.
#[must_use]
pub fn may_mutate(sql: &str) -> bool {
    match leading_keyword(sql) {
        Some(k) if k == "WITH" => {
            let upper = sql.to_ascii_uppercase();
            ["INSERT", "UPDATE", "DELETE", "MERGE"]
                .iter()
                .any(|kw| upper.contains(kw))
        }
        Some(k) => MUTATING.contains(&k.as_str()),
        None => false,
    }
}

/// Engine-aware [`may_mutate`]: SurrealQL and MongoDB input use their own
/// write detection (the drivers' read-only guards), never the SQL keywords.
#[must_use]
pub fn may_mutate_in(engine: DatabaseEngine, text: &str) -> bool {
    match engine {
        DatabaseEngine::Postgres | DatabaseEngine::MySql | DatabaseEngine::Sqlite => {
            may_mutate(text)
        }
        DatabaseEngine::SurrealDb => ferrumdata_db_surrealdb::writes(text),
        DatabaseEngine::MongoDb => ferrumdata_db_mongodb::is_write_query(text),
        // Anything off the read-only allow-list counts as a write.
        DatabaseEngine::Redis => ferrumdata_db_redis::writes(text),
    }
}

/// Whether the UI must ask for explicit confirmation before running.
#[must_use]
pub fn requires_confirmation(profile: &ConnectionProfile, sql: &str) -> bool {
    profile.env.is_production() && may_mutate_in(profile.engine, sql)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::EnvironmentTag;

    #[test]
    fn detects_leading_keyword_through_comments() {
        assert_eq!(
            leading_keyword("  -- hi\n/* x */ (select 1)").as_deref(),
            Some("SELECT")
        );
        assert_eq!(leading_keyword("-- only comment"), None);
    }

    #[test]
    fn classifies_statements() {
        assert!(!may_mutate("select * from t"));
        assert!(may_mutate("delete from t"));
        assert!(may_mutate("  DROP TABLE t"));
        assert!(may_mutate(
            "with x as (delete from t returning *) select * from x"
        ));
        assert!(!may_mutate("with x as (select 1) select * from x"));
    }

    #[test]
    fn surrealql_uses_its_own_keywords() {
        let s = DatabaseEngine::SurrealDb;
        assert!(may_mutate_in(s, "RELATE a:1->likes->b:2"));
        assert!(may_mutate_in(s, "// c\nDEFINE TABLE x"));
        assert!(may_mutate_in(s, "SELECT * FROM t; DELETE t"));
        assert!(!may_mutate_in(s, "SELECT * FROM t"));
        assert!(!may_mutate_in(s, "INFO FOR DB"));
    }

    #[test]
    fn mongo_input_never_hits_sql_keywords() {
        let m = DatabaseEngine::MongoDb;
        // `delete` in a string or field name is not a write.
        assert!(!may_mutate_in(m, "db.logs.find({ action: 'DELETE' })"));
        assert!(!may_mutate_in(m, r#"{ "count": "updates" }"#));
        assert!(may_mutate_in(m, "db.logs.deleteMany({})"));
        assert!(may_mutate_in(m, r#"{ "drop": "logs" }"#));
        // Garbage neither crashes nor counts as a write.
        assert!(!may_mutate_in(m, "DROP TABLE t"));
        assert!(!may_mutate_in(m, "db.("));
        assert!(!may_mutate_in(m, ""));
    }

    #[test]
    fn redis_commands_use_the_read_allow_list() {
        let r = DatabaseEngine::Redis;
        assert!(may_mutate_in(r, "DEL user:1"));
        assert!(may_mutate_in(r, "flushall"));
        assert!(may_mutate_in(r, "CONFIG SET maxmemory 1"));
        assert!(!may_mutate_in(r, "GET delete"));
        assert!(!may_mutate_in(r, "CONFIG GET maxmemory"));
        assert!(!may_mutate_in(r, "SELECT 1"));
        assert!(!may_mutate_in(r, ""));
    }

    #[test]
    fn confirmation_uses_the_profile_engine() {
        let mut p = ConnectionProfile::new(DatabaseEngine::MongoDb, "m");
        p.env = EnvironmentTag::Production;
        assert!(requires_confirmation(
            &p,
            "db.u.updateOne({}, { $set: { a: 1 } })"
        ));
        assert!(!requires_confirmation(&p, "db.u.find({ op: 'update' })"));
    }

    #[test]
    fn confirmation_only_on_production() {
        let mut p = ConnectionProfile::new_postgres("p");
        assert!(!requires_confirmation(&p, "delete from t"));
        p.env = EnvironmentTag::Production;
        assert!(requires_confirmation(&p, "delete from t"));
        assert!(!requires_confirmation(&p, "select 1"));
    }
}
