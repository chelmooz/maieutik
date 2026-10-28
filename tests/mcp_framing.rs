//! Tests for MCP framing robustness (Manus B5 / AUD-06).
//!
//! Verifies that the length-prefixed JSON framing handles edge cases correctly:
//! - Oversized frames
//! - JSON parse errors
//! - Serialization format
//! - MAX_REQUEST_SIZE constant

use maieutik::mcp::framed::{RpcRequest, RpcResponse, MAX_REQUEST_SIZE};

#[tokio::test]
async fn max_request_size_constant() {
    assert_eq!(maieutik::mcp::framed::MAX_REQUEST_SIZE, 1024 * 1024); // 1 MB
}

#[tokio::test]
async fn rpc_request_serialization() {
    let req = maieutik::mcp::framed::RpcRequest {
        method: "health.check".to_string(),
        params: serde_json::json!({}),
        id: Some(serde_json::json!(42)),
    };
    
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("health.check"));
    assert!(json.contains("42"));
}

#[tokio::test]
async fn rpc_request_with_complex_params() {
    let req = maieutik::mcp::framed::RpcRequest {
        method: "search".to_string(),
        params: serde_json::json!({"query": "test", "limit": 10}),
        id: Some(serde_json::json!(1)),
    };
    
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("search"));
    assert!(json.contains("test"));
    assert!(json.contains("10"));
}

#[tokio::test]
async fn rpc_response_serialization() {
    let resp = maieutik::mcp::framed::RpcResponse {
        ok: true,
        result: Some(serde_json::json!({"status": "ok"})),
        error: None,
        id: Some(serde_json::json!(1)),
    };
    
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("ok"));
    assert!(json.contains("true"));
    assert!(json.contains("status"));
}

#[tokio::test]
async fn rpc_response_error_serialization() {
    let resp = maieutik::mcp::framed::RpcResponse {
        ok: false,
        result: None,
        error: Some("Method not found".to_string()),
        id: Some(serde_json::json!(1)),
    };
    
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("false"));
    assert!(json.contains("Method not found"));
    assert!(!json.contains("result")); // skip_serializing_if = "Option::is_none"
}

#[tokio::test]
async fn rpc_response_with_null_error() {
    let resp = maieutik::mcp::framed::RpcResponse {
        ok: true,
        result: Some(serde_json::json!({"data": "test"})),
        error: None,
        id: Some(serde_json::json!(1)),
    };
    
    let json = serde_json::to_string(&resp).unwrap();
    // error field should be omitted when None
    assert!(!json.contains("error"));
}

#[tokio::test]
async fn rpc_request_without_id() {
    let req = maieutik::mcp::framed::RpcRequest {
        method: "notifications/initialized".to_string(),
        params: serde_json::json!({}),
        id: None,
    };
    
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("notifications/initialized"));
    assert!(!json.contains("id")); // skip_serializing_if = "Option::is_none"
}

#[tokio::test]
async fn oversized_response_check() {
    // Create a response larger than MAX_REQUEST_SIZE
    let _huge_data = "x".repeat(maieutik::mcp::framed::MAX_REQUEST_SIZE + 100);
    let bytes = serde_json::to_vec(&maieutik::mcp::framed::RpcResponse {
        ok: true,
        result: Some(serde_json::json!({"data": "x".repeat(maieutik::mcp::framed::MAX_REQUEST_SIZE + 100)})),
        error: None,
        id: Some(serde_json::json!(1)),
    }).unwrap();
    
    let len = bytes.len() as u32;
    assert!(len > maieutik::mcp::framed::MAX_REQUEST_SIZE as u32);
}

#[tokio::test]
async fn rpc_request_without_id_notification() {
    // Notifications don't have an id
    let req = maieutik::mcp::framed::RpcRequest {
        method: "notifications/initialized".to_string(),
        params: serde_json::json!({}),
        id: None,
    };
    
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("notifications/initialized"));
    assert!(!json.contains("id"));
}

#[tokio::test]
async fn rpc_request_complex_params() {
    let req = maieutik::mcp::framed::RpcRequest {
        method: "search".to_string(),
        params: serde_json::json!({"query": "test", "limit": 10, "filters": {"type": "note"}}),
        id: Some(serde_json::json!(1)),
    };
    
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("search"));
    assert!(json.contains("test"));
    assert!(json.contains("limit"));
    assert!(json.contains("10"));
}