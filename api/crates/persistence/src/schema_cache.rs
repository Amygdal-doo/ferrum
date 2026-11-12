//! Cached API descriptions (GraphQL schema, WSDL, OData metadata, gRPC
//! services) keyed by kind and endpoint.

use crate::{Store, StoreResult, now, to_json};
use serde::Serialize;
use serde::de::DeserializeOwned;

impl Store {
    /// `None` when nothing is cached or the cached value no longer parses.
    pub async fn load_schema<T: DeserializeOwned>(
        &self,
        kind: &str,
        endpoint: &str,
    ) -> StoreResult<Option<T>> {
        let json: Option<String> =
            sqlx::query_scalar("SELECT data FROM schema_cache WHERE kind = ? AND endpoint = ?")
                .bind(kind)
                .bind(endpoint)
                .fetch_optional(&self.pool)
                .await?;
        Ok(json.and_then(|j| match serde_json::from_str(&j) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(kind, err = ?e.classify(), "cached schema unreadable; ignoring");
                None
            }
        }))
    }

    pub async fn save_schema<T: Serialize>(
        &self,
        kind: &str,
        endpoint: &str,
        value: &T,
    ) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO schema_cache (kind, endpoint, data, fetched_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(kind, endpoint) DO UPDATE SET data = excluded.data,
                 fetched_at = excluded.fetched_at",
        )
        .bind(kind)
        .bind(endpoint)
        .bind(to_json(value)?)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[tokio::test]
    async fn schemas_are_cached_per_kind_and_endpoint() {
        let store = Store::in_memory().await.expect("store");
        let none: Option<Vec<String>> = store
            .load_schema("graphql", "http://a")
            .await
            .expect("load");
        assert!(none.is_none());
        store
            .save_schema("graphql", "http://a", &vec!["Query".to_owned()])
            .await
            .expect("save");
        store
            .save_schema(
                "graphql",
                "http://a",
                &vec!["Query".to_owned(), "User".to_owned()],
            )
            .await
            .expect("replace");
        let got: Option<Vec<String>> = store
            .load_schema("graphql", "http://a")
            .await
            .expect("load");
        assert_eq!(got, Some(vec!["Query".to_owned(), "User".to_owned()]));
        let other: Option<Vec<String>> = store.load_schema("wsdl", "http://a").await.expect("load");
        assert!(other.is_none());
        let wrong: Option<u32> = store
            .load_schema("graphql", "http://a")
            .await
            .expect("load");
        assert!(wrong.is_none(), "unparseable cache entries are ignored");
    }
}
