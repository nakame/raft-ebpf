//! CLI command implementations

mod client;

use anyhow::Result;
use client::IpcClient;
use serde::{Deserialize, Serialize};

/// IPC request types (must match raft/src/ipc.rs)
#[derive(Debug, Serialize, Deserialize)]
pub enum IpcRequest {
    Join { node_id: String, address: String },
    Leave { node_id: String },
    Status,
    Propose { key: String, value: Option<String> },
    Get { key: String },
    Send { message: String },
    Messages { limit: Option<u64>, since_index: Option<u64> },
}

/// IPC response type
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResponse {
    pub success: bool,
    pub message: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// Join a new node to the cluster
pub async fn join(local_node: &str, node_id: &str, address: &str) -> Result<()> {
    let mut client = IpcClient::connect(local_node).await?;

    let request = IpcRequest::Join {
        node_id: node_id.to_string(),
        address: address.to_string(),
    };

    let response = client.send_request(&request).await?;

    if response.success {
        println!("✓ Successfully joined {} at {}", node_id, address);
    } else {
        eprintln!("✗ Failed to join: {}", response.message);
    }

    Ok(())
}

/// Remove a node from the cluster
pub async fn leave(local_node: &str, node_id: &str) -> Result<()> {
    let mut client = IpcClient::connect(local_node).await?;

    let request = IpcRequest::Leave {
        node_id: node_id.to_string(),
    };

    let response = client.send_request(&request).await?;

    if response.success {
        println!("✓ Successfully removed {}", node_id);
    } else {
        eprintln!("✗ Failed to remove: {}", response.message);
    }

    Ok(())
}

/// Get cluster status
pub async fn status(local_node: &str) -> Result<()> {
    let mut client = IpcClient::connect(local_node).await?;

    let request = IpcRequest::Status;
    let response = client.send_request(&request).await?;

    if response.success {
        if let Some(data) = response.data {
            println!("RAFT Cluster Status");
            println!("==================");

            if let Some(node_id) = data.get("node_id").and_then(|v| v.as_str()) {
                println!("Node ID:      {}", node_id);
            }
            if let Some(role) = data.get("role").and_then(|v| v.as_str()) {
                println!("Role:         {}", role);
            }
            if let Some(term) = data.get("current_term").and_then(|v| v.as_u64()) {
                println!("Current Term: {}", term);
            }
            if let Some(commit) = data.get("commit_index").and_then(|v| v.as_u64()) {
                println!("Commit Index: {}", commit);
            }
            if let Some(applied) = data.get("last_applied").and_then(|v| v.as_u64()) {
                println!("Last Applied: {}", applied);
            }
            if let Some(leader) = data.get("leader_id") {
                if leader.is_null() {
                    println!("Leader:       (none)");
                } else if let Some(leader_str) = leader.as_str() {
                    println!("Leader:       {}", leader_str);
                }
            }
            if let Some(peers) = data.get("peers").and_then(|v| v.as_array()) {
                println!("Peers:        {:?}", peers);
            }
        }
    } else {
        eprintln!("✗ Failed to get status: {}", response.message);
    }

    Ok(())
}

/// Propose a key-value operation
pub async fn propose(
    local_node: &str,
    operation: &str,
    key: &str,
    value: Option<&str>,
) -> Result<()> {
    let mut client = IpcClient::connect(local_node).await?;

    let request = match operation {
        "put" => {
            let value = value.ok_or_else(|| anyhow::anyhow!("Value required for put operation"))?;
            IpcRequest::Propose {
                key: key.to_string(),
                value: Some(value.to_string()),
            }
        }
        "delete" => IpcRequest::Propose {
            key: key.to_string(),
            value: None,
        },
        "get" => IpcRequest::Get {
            key: key.to_string(),
        },
        _ => anyhow::bail!("Unknown operation: {}", operation),
    };

    let response = client.send_request(&request).await?;

    if response.success {
        match operation {
            "put" => println!("✓ Set {}={}", key, value.unwrap_or("")),
            "delete" => println!("✓ Deleted {}", key),
            "get" => {
                if let Some(data) = response.data {
                    if let Some(v) = data.get("value") {
                        if v.is_null() {
                            println!("{}: (not found)", key);
                        } else if let Some(s) = v.as_str() {
                            println!("{}: {}", key, s);
                        }
                    }
                }
            }
            _ => {}
        }
    } else {
        eprintln!("✗ Operation failed: {}", response.message);
        if let Some(data) = response.data {
            if let Some(leader) = data.get("leader_id").and_then(|v| v.as_str()) {
                eprintln!("  Hint: Try connecting to leader node: {}", leader);
            }
        }
    }

    Ok(())
}

/// Send a message to the cluster
pub async fn send(local_node: &str, message: &str) -> Result<()> {
    let mut client = IpcClient::connect(local_node).await?;

    let request = IpcRequest::Send {
        message: message.to_string(),
    };

    let response = client.send_request(&request).await?;

    if response.success {
        println!("✓ Message sent: \"{}\"", message);
        if let Some(data) = response.data {
            if let Some(index) = data.get("index").and_then(|v| v.as_u64()) {
                println!("  Replicated at log index {}", index);
            }
        }
    } else {
        eprintln!("✗ Failed to send: {}", response.message);
        if let Some(data) = response.data {
            if let Some(leader) = data.get("leader_id").and_then(|v| v.as_str()) {
                eprintln!("  Hint: Send from leader node: {}", leader);
            }
        }
    }

    Ok(())
}

/// Get recent messages from the cluster
pub async fn messages(local_node: &str, limit: u64) -> Result<()> {
    let mut client = IpcClient::connect(local_node).await?;

    let request = IpcRequest::Messages {
        limit: Some(limit),
        since_index: None,
    };

    let response = client.send_request(&request).await?;

    if response.success {
        if let Some(data) = response.data {
            if let Some(last_applied) = data.get("last_applied").and_then(|v| v.as_u64()) {
                println!("Messages (last applied index: {})", last_applied);
                println!("{}", "=".repeat(50));
            }

            if let Some(messages) = data.get("messages").and_then(|v| v.as_array()) {
                if messages.is_empty() {
                    println!("(no messages)");
                } else {
                    for msg in messages {
                        let from = msg.get("from").and_then(|v| v.as_str()).unwrap_or("?");
                        let text = msg.get("message").and_then(|v| v.as_str()).unwrap_or("");
                        let ts = msg.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0);

                        // Format timestamp
                        let secs = ts / 1000;
                        let datetime = chrono_lite(secs);

                        println!("[{}] {}: {}", datetime, from, text);
                    }
                }
            }
        }
    } else {
        eprintln!("✗ Failed to get messages: {}", response.message);
    }

    Ok(())
}

/// Watch for new messages in real-time
pub async fn watch(local_node: &str, interval_ms: u64) -> Result<()> {
    println!("Watching for messages on {} (Ctrl+C to stop)...", local_node);
    println!("{}", "=".repeat(50));

    let mut seen_messages = std::collections::HashSet::new();
    let interval = std::time::Duration::from_millis(interval_ms);

    loop {
        let mut client = IpcClient::connect(local_node).await?;

        let request = IpcRequest::Messages {
            limit: Some(50),
            since_index: None,
        };

        let response = client.send_request(&request).await?;

        if response.success {
            if let Some(data) = response.data {
                if let Some(messages) = data.get("messages").and_then(|v| v.as_array()) {
                    // Process in reverse order (oldest first) for display
                    for msg in messages.iter().rev() {
                        let ts = msg.get("timestamp").and_then(|v| v.as_u64()).unwrap_or(0);
                        let text = msg.get("message").and_then(|v| v.as_str()).unwrap_or("");
                        let key = format!("{}:{}", ts, text);

                        if !seen_messages.contains(&key) {
                            seen_messages.insert(key);
                            let from = msg.get("from").and_then(|v| v.as_str()).unwrap_or("?");
                            let datetime = chrono_lite(ts / 1000);
                            println!("[{}] {}: {}", datetime, from, text);
                        }
                    }
                }
            }
        }

        tokio::time::sleep(interval).await;
    }
}

/// Simple timestamp formatter (no chrono dependency)
fn chrono_lite(unix_secs: u64) -> String {
    // Just return the unix timestamp in a readable format
    // For a full implementation, you'd use the chrono crate
    let secs = unix_secs % 60;
    let mins = (unix_secs / 60) % 60;
    let hours = (unix_secs / 3600) % 24;
    format!("{:02}:{:02}:{:02}", hours, mins, secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_request_join_serialization() {
        let request = IpcRequest::Join {
            node_id: "node-02".to_string(),
            address: "192.168.1.2:5555".to_string(),
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: IpcRequest = serde_json::from_str(&json).expect("deserialize");

        if let IpcRequest::Join { node_id, address } = parsed {
            assert_eq!(node_id, "node-02");
            assert_eq!(address, "192.168.1.2:5555");
        } else {
            panic!("Expected Join request");
        }
    }

    #[test]
    fn test_ipc_request_leave_serialization() {
        let request = IpcRequest::Leave {
            node_id: "node-03".to_string(),
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: IpcRequest = serde_json::from_str(&json).expect("deserialize");

        if let IpcRequest::Leave { node_id } = parsed {
            assert_eq!(node_id, "node-03");
        } else {
            panic!("Expected Leave request");
        }
    }

    #[test]
    fn test_ipc_request_status_serialization() {
        let request = IpcRequest::Status;
        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: IpcRequest = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(parsed, IpcRequest::Status));
    }

    #[test]
    fn test_ipc_request_propose_put_serialization() {
        let request = IpcRequest::Propose {
            key: "mykey".to_string(),
            value: Some("myvalue".to_string()),
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: IpcRequest = serde_json::from_str(&json).expect("deserialize");

        if let IpcRequest::Propose { key, value } = parsed {
            assert_eq!(key, "mykey");
            assert_eq!(value, Some("myvalue".to_string()));
        } else {
            panic!("Expected Propose request");
        }
    }

    #[test]
    fn test_ipc_request_propose_delete_serialization() {
        let request = IpcRequest::Propose {
            key: "deletekey".to_string(),
            value: None,
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: IpcRequest = serde_json::from_str(&json).expect("deserialize");

        if let IpcRequest::Propose { key, value } = parsed {
            assert_eq!(key, "deletekey");
            assert!(value.is_none());
        } else {
            panic!("Expected Propose request");
        }
    }

    #[test]
    fn test_ipc_request_get_serialization() {
        let request = IpcRequest::Get {
            key: "lookupkey".to_string(),
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: IpcRequest = serde_json::from_str(&json).expect("deserialize");

        if let IpcRequest::Get { key } = parsed {
            assert_eq!(key, "lookupkey");
        } else {
            panic!("Expected Get request");
        }
    }

    #[test]
    fn test_ipc_request_send_serialization() {
        let request = IpcRequest::Send {
            message: "Hello, RAFT cluster!".to_string(),
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: IpcRequest = serde_json::from_str(&json).expect("deserialize");

        if let IpcRequest::Send { message } = parsed {
            assert_eq!(message, "Hello, RAFT cluster!");
        } else {
            panic!("Expected Send request");
        }
    }

    #[test]
    fn test_ipc_request_messages_serialization() {
        let request = IpcRequest::Messages {
            limit: Some(50),
            since_index: Some(100),
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: IpcRequest = serde_json::from_str(&json).expect("deserialize");

        if let IpcRequest::Messages { limit, since_index } = parsed {
            assert_eq!(limit, Some(50));
            assert_eq!(since_index, Some(100));
        } else {
            panic!("Expected Messages request");
        }
    }

    #[test]
    fn test_ipc_request_messages_no_options_serialization() {
        let request = IpcRequest::Messages {
            limit: None,
            since_index: None,
        };

        let json = serde_json::to_string(&request).expect("serialize");
        let parsed: IpcRequest = serde_json::from_str(&json).expect("deserialize");

        if let IpcRequest::Messages { limit, since_index } = parsed {
            assert!(limit.is_none());
            assert!(since_index.is_none());
        } else {
            panic!("Expected Messages request");
        }
    }

    #[test]
    fn test_ipc_response_success_serialization() {
        let response = IpcResponse {
            success: true,
            message: "Operation completed".to_string(),
            data: Some(serde_json::json!({"index": 42})),
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let parsed: IpcResponse = serde_json::from_str(&json).expect("deserialize");

        assert!(parsed.success);
        assert_eq!(parsed.message, "Operation completed");
        assert!(parsed.data.is_some());
        assert_eq!(parsed.data.unwrap()["index"], 42);
    }

    #[test]
    fn test_ipc_response_failure_serialization() {
        let response = IpcResponse {
            success: false,
            message: "Not leader".to_string(),
            data: Some(serde_json::json!({"leader_id": "node-02"})),
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let parsed: IpcResponse = serde_json::from_str(&json).expect("deserialize");

        assert!(!parsed.success);
        assert_eq!(parsed.message, "Not leader");
        assert_eq!(parsed.data.unwrap()["leader_id"], "node-02");
    }

    #[test]
    fn test_ipc_response_no_data_serialization() {
        let response = IpcResponse {
            success: true,
            message: "OK".to_string(),
            data: None,
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let parsed: IpcResponse = serde_json::from_str(&json).expect("deserialize");

        assert!(parsed.success);
        assert!(parsed.data.is_none());
    }

    #[test]
    fn test_chrono_lite_midnight() {
        let result = chrono_lite(0);
        assert_eq!(result, "00:00:00");
    }

    #[test]
    fn test_chrono_lite_noon() {
        let result = chrono_lite(12 * 3600); // 12 hours in seconds
        assert_eq!(result, "12:00:00");
    }

    #[test]
    fn test_chrono_lite_specific_time() {
        // 14:35:42 = 14*3600 + 35*60 + 42 = 52542 seconds
        let result = chrono_lite(52542);
        assert_eq!(result, "14:35:42");
    }

    #[test]
    fn test_chrono_lite_wraps_at_24_hours() {
        // 25 hours wraps to 01:00:00
        let result = chrono_lite(25 * 3600);
        assert_eq!(result, "01:00:00");
    }

    #[test]
    fn test_chrono_lite_end_of_day() {
        // 23:59:59
        let result = chrono_lite(23 * 3600 + 59 * 60 + 59);
        assert_eq!(result, "23:59:59");
    }
}
