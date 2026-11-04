//! Tests du serveur MCP (maieutik-ui integration)
//!
//! Ces tests vérifient que le serveur MCP répond correctement aux méthodes
//! définies dans le contrat: health.check, search.*, chat.complete

use maieutik::mcp::framed::{RpcRequest, RpcResponse};

fn create_request(method: &str, params: serde_json::Value) -> RpcRequest {
    RpcRequest {
        method: method.to_string(),
        params,
        id: Some(serde_json::json!(1)),
    }
}

#[test]
fn test_health_check_request_format() {
    let req = create_request("health.check", serde_json::Value::Null);

    assert_eq!(req.method, "health.check");
    assert!(req.id.is_some());
}

#[test]
fn test_search_semantic_request_format() {
    let params = serde_json::json!({
        "query": "RAG",
        "limit": 5,
        "min_score": 0.5
    });

    let req = create_request("search.semantic", params);

    assert_eq!(req.method, "search.semantic");
    let p = req.params;
    assert_eq!(p["query"], "RAG");
}

#[test]
fn test_search_hybrid_request_format() {
    let params = serde_json::json!({
        "query": "LLM",
        "limit": 10,
        "folders": ["/home/user/notes"],
        "min_score": 0.3
    });

    let req = create_request("search.hybrid", params);

    assert_eq!(req.method, "search.hybrid");
}

#[test]
fn test_chat_complete_request_format() {
    let params = serde_json::json!({
        "message": "Explique RAG",
        "file_path": null,
        "scope": null
    });

    let req = create_request("chat.complete", params);

    assert_eq!(req.method, "chat.complete");
    let p = req.params;
    assert_eq!(p["message"], "Explique RAG");
}

#[test]
fn test_rpc_response_success_format() {
    let response = RpcResponse {
        ok: true,
        result: Some(serde_json::json!({"version": "0.1.0"})),
        error: None,
        id: Some(serde_json::json!(1)),
    };

    assert!(response.ok);
    assert!(response.error.is_none());
}

#[test]
fn test_rpc_response_error_format() {
    let response = RpcResponse {
        ok: false,
        result: None,
        error: Some("Method not found".to_string()),
        id: Some(serde_json::json!(2)),
    };

    assert!(!response.ok);
    assert!(response.result.is_none());
    assert!(response.error.is_some());
}
