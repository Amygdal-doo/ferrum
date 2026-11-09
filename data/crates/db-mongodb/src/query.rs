//! The MongoDB query input accepted by the editor.
//!
//! Two forms:
//!
//! 1. A mongo-shell subset, one call per run:
//!    `db.<collection>.<method>(<args>)`, where the collection may also be
//!    written `db.getCollection("name")` or `db["name"]`. Methods:
//!    `find(filter?, projection?)` with optional `.sort({…})`, `.limit(n)`,
//!    `.skip(n)`, `.projection({…})`; `findOne(filter?, projection?)`;
//!    `aggregate([stages])`; `countDocuments(filter?)`;
//!    `estimatedDocumentCount()`; `distinct("field", filter?)`;
//!    `insertOne(doc)`, `insertMany([docs])`, `updateOne/updateMany(filter, update)`,
//!    `replaceOne(filter, doc)`, `deleteOne/deleteMany(filter)`.
//! 2. A database command as a JSON document, run with `runCommand`, e.g.
//!    `{ "count": "users", "query": { "age": { "$gt": 30 } } }`.
//!
//! Arguments use the relaxed syntax of [`crate::relaxed_json`].

use crate::relaxed_json::{ParseError, Reader};
use bson::{Bson, Document};

#[derive(Debug, Clone, PartialEq)]
pub enum MongoQuery {
    Find {
        collection: String,
        filter: Document,
        projection: Option<Document>,
        sort: Option<Document>,
        limit: Option<i64>,
        skip: Option<u64>,
        /// `findOne`: at most one document.
        one: bool,
    },
    Aggregate {
        collection: String,
        pipeline: Vec<Document>,
    },
    Count {
        collection: String,
        filter: Document,
    },
    EstimatedCount {
        collection: String,
    },
    Distinct {
        collection: String,
        field: String,
        filter: Document,
    },
    InsertOne {
        collection: String,
        document: Document,
    },
    InsertMany {
        collection: String,
        documents: Vec<Document>,
    },
    Update {
        collection: String,
        filter: Document,
        update: Document,
        many: bool,
    },
    Replace {
        collection: String,
        filter: Document,
        replacement: Document,
    },
    Delete {
        collection: String,
        filter: Document,
        many: bool,
    },
    /// `db.runCommand(<document>)`.
    Command(Document),
}

/// Commands that write data or change the schema (first key of a command document).
const WRITING_COMMANDS: &[&str] = &[
    "insert",
    "update",
    "delete",
    "findAndModify",
    "findandmodify",
    "drop",
    "dropDatabase",
    "create",
    "createIndexes",
    "dropIndexes",
    "renameCollection",
    "collMod",
    "bulkWrite",
    "createUser",
    "dropUser",
    "updateUser",
    "grantRolesToUser",
    "revokeRolesFromUser",
];

impl MongoQuery {
    /// True if running it may modify data or schema: write methods, writing
    /// commands, and pipelines ending in `$out` / `$merge`.
    #[must_use]
    pub fn is_write(&self) -> bool {
        match self {
            Self::Find { .. }
            | Self::Count { .. }
            | Self::EstimatedCount { .. }
            | Self::Distinct { .. } => false,
            Self::Aggregate { pipeline, .. } => pipeline_writes(pipeline),
            Self::InsertOne { .. }
            | Self::InsertMany { .. }
            | Self::Update { .. }
            | Self::Replace { .. }
            | Self::Delete { .. } => true,
            Self::Command(doc) => {
                let first = doc.keys().next().map(String::as_str).unwrap_or_default();
                WRITING_COMMANDS.contains(&first)
                    || (first == "aggregate"
                        && doc
                            .get_array("pipeline")
                            .is_ok_and(|p| p.iter().any(stage_writes)))
            }
        }
    }
}

fn pipeline_writes(pipeline: &[Document]) -> bool {
    pipeline
        .iter()
        .any(|s| s.contains_key("$out") || s.contains_key("$merge"))
}

fn stage_writes(stage: &Bson) -> bool {
    stage
        .as_document()
        .is_some_and(|s| s.contains_key("$out") || s.contains_key("$merge"))
}

/// Parses the editor text into a query.
pub fn parse(input: &str) -> Result<MongoQuery, ParseError> {
    let mut r = Reader::new(input);
    r.skip_ws();
    let query = if r.peek() == Some('{') {
        MongoQuery::Command(document(&mut r, "a command document")?)
    } else {
        shell(&mut r)?
    };
    r.eat(';');
    r.skip_ws();
    if r.peek().is_some() {
        return r.error("unexpected text after the query (run one call at a time)");
    }
    Ok(query)
}

fn shell(r: &mut Reader<'_>) -> Result<MongoQuery, ParseError> {
    if r.ident() != Some("db") {
        return r.error(
            "expected db.<collection>.<method>(…) or a JSON command such as { \"ping\": 1 }",
        );
    }
    let collection = if r.eat('[') {
        let name = r.string()?;
        r.expect(']')?;
        r.expect('.')?;
        name
    } else {
        r.expect('.')?;
        match r.ident() {
            Some("getCollection") => {
                r.expect('(')?;
                let name = r.string()?;
                r.expect(')')?;
                r.expect('.')?;
                name
            }
            Some("runCommand") => {
                r.expect('(')?;
                let doc = document(r, "a command document")?;
                r.expect(')')?;
                return Ok(MongoQuery::Command(doc));
            }
            Some(name) => {
                let name = name.to_owned();
                r.expect('.')?;
                name
            }
            None => return r.error("expected a collection name"),
        }
    };
    let Some(method) = r.ident() else {
        return r.error("expected a method such as find(…)");
    };
    let method = method.to_owned();
    let at = r.pos();
    let mut args = Args::read(r)?;
    let query = match method.as_str() {
        "find" | "findOne" => {
            let filter = args.optional_document("the filter")?.unwrap_or_default();
            let projection = args.optional_document("the projection")?;
            args.done()?;
            let mut q = MongoQuery::Find {
                collection,
                filter,
                projection,
                sort: None,
                limit: None,
                skip: None,
                one: method == "findOne",
            };
            if method == "find" {
                cursor_modifiers(r, &mut q)?;
            }
            q
        }
        "aggregate" => {
            let pipeline = args.documents("the pipeline")?;
            args.done()?;
            MongoQuery::Aggregate {
                collection,
                pipeline,
            }
        }
        "countDocuments" => {
            let filter = args.optional_document("the filter")?.unwrap_or_default();
            args.done()?;
            MongoQuery::Count { collection, filter }
        }
        "estimatedDocumentCount" => {
            args.done()?;
            MongoQuery::EstimatedCount { collection }
        }
        "distinct" => {
            let field = args.string("the field name")?;
            let filter = args.optional_document("the filter")?.unwrap_or_default();
            args.done()?;
            MongoQuery::Distinct {
                collection,
                field,
                filter,
            }
        }
        "insertOne" => {
            let document = args.document("the document")?;
            args.done()?;
            MongoQuery::InsertOne {
                collection,
                document,
            }
        }
        "insertMany" => {
            let documents = args.documents("the documents")?;
            args.done()?;
            MongoQuery::InsertMany {
                collection,
                documents,
            }
        }
        "updateOne" | "updateMany" => {
            let filter = args.document("the filter")?;
            let update = args.document("the update")?;
            args.done()?;
            MongoQuery::Update {
                collection,
                filter,
                update,
                many: method == "updateMany",
            }
        }
        "replaceOne" => {
            let filter = args.document("the filter")?;
            let replacement = args.document("the replacement")?;
            args.done()?;
            MongoQuery::Replace {
                collection,
                filter,
                replacement,
            }
        }
        "deleteOne" | "deleteMany" => {
            let filter = args.document("the filter")?;
            args.done()?;
            MongoQuery::Delete {
                collection,
                filter,
                many: method == "deleteMany",
            }
        }
        other => {
            return Err(ParseError {
                message: format!(
                    "unsupported method {other}(); use find, findOne, aggregate, countDocuments, \
                     estimatedDocumentCount, distinct, insertOne/Many, updateOne/Many, replaceOne, \
                     deleteOne/Many, or a JSON command"
                ),
                at,
            });
        }
    };
    Ok(query)
}

/// `.sort({…})`, `.limit(n)`, `.skip(n)`, `.projection({…})` after `find(…)`.
/// `.toArray()` and `.pretty()` are accepted and ignored.
fn cursor_modifiers(r: &mut Reader<'_>, q: &mut MongoQuery) -> Result<(), ParseError> {
    let MongoQuery::Find {
        sort,
        limit,
        skip,
        projection,
        ..
    } = q
    else {
        return Ok(());
    };
    while r.eat('.') {
        let at = r.pos();
        let Some(name) = r.ident() else {
            return r.error("expected a cursor method such as sort(…)");
        };
        let name = name.to_owned();
        let mut args = Args::read(r)?;
        match name.as_str() {
            "sort" => *sort = Some(args.document("the sort")?),
            "projection" => *projection = Some(args.document("the projection")?),
            "limit" => *limit = Some(args.integer("the limit")?),
            "skip" => {
                *skip = Some(
                    u64::try_from(args.integer("the skip")?).map_err(|_| ParseError {
                        message: "skip must not be negative".into(),
                        at,
                    })?,
                );
            }
            "toArray" | "pretty" => {}
            other => {
                return Err(ParseError {
                    message: format!(
                        "unsupported cursor method {other}(); use sort, limit, skip or projection"
                    ),
                    at,
                });
            }
        }
        args.done()?;
    }
    Ok(())
}

/// Call arguments, already parsed, consumed left to right.
struct Args {
    values: std::vec::IntoIter<(usize, serde_json::Value)>,
    end: usize,
}

impl Args {
    fn read(r: &mut Reader<'_>) -> Result<Self, ParseError> {
        r.expect('(')?;
        let mut values = Vec::new();
        if !r.eat(')') {
            loop {
                r.skip_ws();
                values.push((r.pos(), r.value()?));
                if !r.eat(',') {
                    r.expect(')')?;
                    break;
                }
                if r.eat(')') {
                    break;
                }
            }
        }
        Ok(Self {
            values: values.into_iter(),
            end: r.pos(),
        })
    }

    fn next(&mut self, what: &str) -> Result<(usize, serde_json::Value), ParseError> {
        self.values.next().ok_or_else(|| ParseError {
            message: format!("missing {what}"),
            at: self.end,
        })
    }

    fn document(&mut self, what: &str) -> Result<Document, ParseError> {
        let (at, v) = self.next(what)?;
        to_document(v, what, at)
    }

    fn optional_document(&mut self, what: &str) -> Result<Option<Document>, ParseError> {
        match self.values.next() {
            None => Ok(None),
            Some((at, v)) => to_document(v, what, at).map(Some),
        }
    }

    fn documents(&mut self, what: &str) -> Result<Vec<Document>, ParseError> {
        let (at, v) = self.next(what)?;
        let serde_json::Value::Array(items) = v else {
            return Err(ParseError {
                message: format!("{what} must be an array of documents"),
                at,
            });
        };
        items
            .into_iter()
            .map(|item| to_document(item, what, at))
            .collect()
    }

    fn string(&mut self, what: &str) -> Result<String, ParseError> {
        match self.next(what)? {
            (_, serde_json::Value::String(s)) => Ok(s),
            (at, _) => Err(ParseError {
                message: format!("{what} must be a string"),
                at,
            }),
        }
    }

    fn integer(&mut self, what: &str) -> Result<i64, ParseError> {
        match self.next(what)? {
            (_, serde_json::Value::Number(n)) if n.is_i64() => Ok(n.as_i64().unwrap_or_default()),
            (at, _) => Err(ParseError {
                message: format!("{what} must be an integer"),
                at,
            }),
        }
    }

    fn done(&mut self) -> Result<(), ParseError> {
        match self.values.next() {
            None => Ok(()),
            Some((at, _)) => Err(ParseError {
                message: "too many arguments".into(),
                at,
            }),
        }
    }
}

fn document(r: &mut Reader<'_>, what: &str) -> Result<Document, ParseError> {
    r.skip_ws();
    let at = r.pos();
    let v = r.value()?;
    to_document(v, what, at)
}

/// Converts relaxed JSON to a BSON document, interpreting Extended JSON
/// (`{"$oid": …}`, `{"$date": …}`, …).
fn to_document(v: serde_json::Value, what: &str, at: usize) -> Result<Document, ParseError> {
    if !v.is_object() {
        return Err(ParseError {
            message: format!("{what} must be a document {{ … }}"),
            at,
        });
    }
    match Bson::try_from(v) {
        Ok(Bson::Document(d)) => Ok(d),
        Ok(_) => Err(ParseError {
            message: format!("{what} must be a document, not an Extended JSON value"),
            at,
        }),
        Err(e) => Err(ParseError {
            message: format!("invalid {what}: {e}"),
            at,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use bson::oid::ObjectId;

    fn ok(src: &str) -> MongoQuery {
        parse(src).unwrap_or_else(|e| panic!("{src}: {e}"))
    }

    #[test]
    fn find_with_cursor_modifiers() {
        let q = ok(
            "db.users.find({ age: { $gt: 30 } }, { name: 1 }).sort({ name: -1 }).skip(5).limit(20);",
        );
        assert_eq!(
            q,
            MongoQuery::Find {
                collection: "users".into(),
                filter: doc! { "age": { "$gt": 30 } },
                projection: Some(doc! { "name": 1 }),
                sort: Some(doc! { "name": -1 }),
                limit: Some(20),
                skip: Some(5),
                one: false,
            }
        );
        assert!(!q.is_write());
    }

    #[test]
    fn collection_name_forms() {
        for src in [
            "db.getCollection('my.coll').find()",
            "db['my.coll'].find({})",
            "  // comment\n db[\"my.coll\"].find( )",
        ] {
            match ok(src) {
                MongoQuery::Find { collection, .. } => assert_eq!(collection, "my.coll"),
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn extended_json_values_become_typed_bson() {
        let q = ok("db.c.findOne({ _id: ObjectId('65a1b2c3d4e5f60718293a4b') })");
        let MongoQuery::Find { filter, one, .. } = q else {
            panic!("not find");
        };
        assert!(one);
        assert_eq!(
            filter.get_object_id("_id").ok(),
            ObjectId::parse_str("65a1b2c3d4e5f60718293a4b").ok()
        );
    }

    #[test]
    fn aggregate_count_distinct() {
        assert_eq!(
            ok(
                "db.o.aggregate([{ $match: { s: 'A' } }, { $group: { _id: '$c', n: { $sum: 1 } } }])"
            ),
            MongoQuery::Aggregate {
                collection: "o".into(),
                pipeline: vec![
                    doc! { "$match": { "s": "A" } },
                    doc! { "$group": { "_id": "$c", "n": { "$sum": 1 } } }
                ],
            }
        );
        assert_eq!(
            ok("db.o.countDocuments()"),
            MongoQuery::Count {
                collection: "o".into(),
                filter: doc! {}
            }
        );
        assert_eq!(
            ok("db.o.distinct('city', { a: 1 })"),
            MongoQuery::Distinct {
                collection: "o".into(),
                field: "city".into(),
                filter: doc! { "a": 1 }
            }
        );
    }

    #[test]
    fn writes_are_detected() {
        assert!(ok("db.u.insertOne({ a: 1 })").is_write());
        assert!(ok("db.u.updateMany({}, { $set: { a: 1 } })").is_write());
        assert!(ok("db.u.deleteOne({ a: 1 })").is_write());
        assert!(ok("db.u.aggregate([{ $match: {} }, { $out: 'x' }])").is_write());
        assert!(!ok("db.u.aggregate([{ $match: {} }])").is_write());
        assert!(ok(r#"{ "drop": "u" }"#).is_write());
        assert!(
            ok(r#"{ "aggregate": "u", "pipeline": [{ "$merge": "x" }], "cursor": {} }"#).is_write()
        );
        assert!(!ok(r#"{ "count": "u" }"#).is_write());
        assert!(!ok("db.runCommand({ ping: 1 })").is_write());
    }

    #[test]
    fn json_commands_keep_key_order() {
        let MongoQuery::Command(cmd) =
            ok(r#"{ "find": "users", "filter": { "a": 1 }, "limit": 2 }"#)
        else {
            panic!("not a command");
        };
        let keys: Vec<_> = cmd.keys().map(String::as_str).collect();
        assert_eq!(keys, ["find", "filter", "limit"]);
    }

    #[test]
    fn helpful_errors() {
        let msg = |s: &str| parse(s).expect_err(s).message;
        assert!(msg("SELECT * FROM users").contains("db.<collection>"));
        assert!(msg("db.users.mapReduce()").contains("unsupported method"));
        assert!(msg("db.users.find().explain()").contains("unsupported cursor method"));
        assert!(msg("db.users.find(1)").contains("must be a document"));
        assert!(msg("db.users.find({}, {}, {})").contains("too many"));
        assert!(msg("db.users.insertOne()").contains("missing"));
        assert!(msg("db.users.find(); db.users.find()").contains("one call"));
        assert!(msg("db.users.find().limit('x')").contains("integer"));
    }
}
