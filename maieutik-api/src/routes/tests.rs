#[cfg(test)]
mod tests {
    use super::super::{chat, health, list_files, search, ChatRequest, SearchParams, SearchResult};
    use axum::http::StatusCode;
    use axum::{
        extract::Query,
        routing::{get, post},
        Json, Router,
    };
    use serde_json::json;

    // Tests unitaires des handlers autonomes (pour la logique pure)

    #[tokio::test]
    async fn health_returns_ok() {
        let res: Json<super::super::Health> = health().await;
        assert_eq!(res.0.status, "ok");
    }

    #[tokio::test]
    async fn search_params_deserialize_correctly() {
        let params: SearchParams = serde_json::from_str(r#"{"query":"test"}"#).unwrap();
        assert_eq!(params.query, "test");
    }
}
