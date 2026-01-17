//! IPC server for CLI communication via Unix socket

use crate::node::{NodeCommand, NodeStatus, ProposeResult, RaftNode};
use anyhow::Result;
use raft_common::Command;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// IPC request from CLI
#[derive(Debug, Serialize, Deserialize)]
pub enum IpcRequest {
    /// Join a new peer to the cluster
    Join { node_id: String, address: String },
    /// Remove a peer from the cluster
    Leave { node_id: String },
    /// Get cluster status
    Status,
    /// Propose a command
    Propose { key: String, value: Option<String> },
    /// Get a value
    Get { key: String },
    /// Send a message to the cluster
    Send { message: String },
    /// Get recent messages
    Messages { limit: Option<u64>, since_index: Option<u64> },
}

/// IPC response to CLI
#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// IPC server for CLI communication
pub struct IpcServer {
    node: Arc<RaftNode>,
    socket_path: PathBuf,
}

impl IpcServer {
    /// Create a new IPC server
    pub fn new(node: Arc<RaftNode>, socket_path: PathBuf) -> Self {
        Self { node, socket_path }
    }

    /// Run the IPC server
    pub async fn run(self) -> Result<()> {
        // Remove existing socket
        let _ = std::fs::remove_file(&self.socket_path);

        // Create parent directory if needed
        if let Some(parent) = self.socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        info!("IPC server listening on {:?}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let node = self.node.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_ipc_connection(stream, node).await {
                            warn!("IPC connection error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("IPC accept error: {}", e);
                }
            }
        }
    }
}

/// Handle a single IPC connection
async fn handle_ipc_connection(stream: UnixStream, node: Arc<RaftNode>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let request: IpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let response = IpcResponse {
                    success: false,
                    message: format!("Invalid request: {}", e),
                    data: None,
                };
                let resp_str = serde_json::to_string(&response)? + "\n";
                writer.write_all(resp_str.as_bytes()).await?;
                line.clear();
                continue;
            }
        };

        let response = process_request(request, &node).await;
        let resp_str = serde_json::to_string(&response)? + "\n";
        writer.write_all(resp_str.as_bytes()).await?;
        writer.flush().await?;
        line.clear();
    }

    Ok(())
}

/// Process an IPC request
async fn process_request(request: IpcRequest, node: &Arc<RaftNode>) -> IpcResponse {
    match request {
        IpcRequest::Join { node_id, address } => {
            node.add_peer(node_id.clone(), address.clone()).await;
            IpcResponse {
                success: true,
                message: format!("Added peer {} at {}", node_id, address),
                data: None,
            }
        }
        IpcRequest::Leave { node_id } => {
            node.remove_peer(&node_id).await;
            IpcResponse {
                success: true,
                message: format!("Removed peer {}", node_id),
                data: None,
            }
        }
        IpcRequest::Status => {
            let status = node.get_status().await;
            IpcResponse {
                success: true,
                message: "OK".to_string(),
                data: Some(serde_json::to_value(&StatusResponse::from(status)).unwrap()),
            }
        }
        IpcRequest::Propose { key, value } => {
            let command = if let Some(v) = value {
                let mut key_str = heapless::String::new();
                let _ = key_str.push_str(&key);
                let mut value_vec = heapless::Vec::new();
                for b in v.as_bytes() {
                    let _ = value_vec.push(*b);
                }
                Command::Put {
                    key: key_str,
                    value: value_vec,
                }
            } else {
                let mut key_str = heapless::String::new();
                let _ = key_str.push_str(&key);
                Command::Delete { key: key_str }
            };

            let (tx, mut rx) = mpsc::channel(1);
            if let Err(e) = node.propose(command, tx).await {
                return IpcResponse {
                    success: false,
                    message: format!("Propose failed: {}", e),
                    data: None,
                };
            }

            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                rx.recv(),
            )
            .await
            {
                Ok(Some(ProposeResult::Success { index })) => IpcResponse {
                    success: true,
                    message: format!("Committed at index {}", index),
                    data: None,
                },
                Ok(Some(ProposeResult::NotLeader { leader_id })) => IpcResponse {
                    success: false,
                    message: "Not leader".to_string(),
                    data: leader_id.map(|id| serde_json::json!({ "leader_id": id })),
                },
                Ok(Some(ProposeResult::Failed { reason })) => IpcResponse {
                    success: false,
                    message: reason,
                    data: None,
                },
                Ok(None) => IpcResponse {
                    success: false,
                    message: "Channel closed".to_string(),
                    data: None,
                },
                Err(_) => IpcResponse {
                    success: false,
                    message: "Timeout waiting for commit".to_string(),
                    data: None,
                },
            }
        }
        IpcRequest::Get { key } => {
            let value = node.get_value(&key).await;
            IpcResponse {
                success: true,
                message: "OK".to_string(),
                data: Some(serde_json::json!({
                    "key": key,
                    "value": value.map(|v| String::from_utf8_lossy(&v).to_string()),
                })),
            }
        }
        IpcRequest::Send { message } => {
            // Create a message with timestamp and sender info
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            let msg_key = format!("_msg:{}", timestamp);
            let msg_value = serde_json::json!({
                "from": node.node_id,
                "timestamp": timestamp,
                "message": message,
            });

            let mut key_str = heapless::String::new();
            let _ = key_str.push_str(&msg_key);
            let mut value_vec = heapless::Vec::new();
            for b in msg_value.to_string().as_bytes() {
                let _ = value_vec.push(*b);
            }
            let command = Command::Put {
                key: key_str,
                value: value_vec,
            };

            let (tx, mut rx) = mpsc::channel(1);
            if let Err(e) = node.propose(command, tx).await {
                return IpcResponse {
                    success: false,
                    message: format!("Send failed: {}", e),
                    data: None,
                };
            }

            match tokio::time::timeout(
                std::time::Duration::from_secs(5),
                rx.recv(),
            )
            .await
            {
                Ok(Some(ProposeResult::Success { index })) => {
                    info!("Message sent: \"{}\" (index {})", message, index);
                    IpcResponse {
                        success: true,
                        message: format!("Message sent (index {})", index),
                        data: Some(serde_json::json!({ "index": index })),
                    }
                }
                Ok(Some(ProposeResult::NotLeader { leader_id })) => IpcResponse {
                    success: false,
                    message: "Not leader - send to leader node".to_string(),
                    data: leader_id.map(|id| serde_json::json!({ "leader_id": id })),
                },
                Ok(Some(ProposeResult::Failed { reason })) => IpcResponse {
                    success: false,
                    message: reason,
                    data: None,
                },
                Ok(None) => IpcResponse {
                    success: false,
                    message: "Channel closed".to_string(),
                    data: None,
                },
                Err(_) => IpcResponse {
                    success: false,
                    message: "Timeout waiting for replication".to_string(),
                    data: None,
                },
            }
        }
        IpcRequest::Messages { limit, since_index } => {
            let messages = node.get_messages(limit.unwrap_or(10), since_index.unwrap_or(0)).await;
            IpcResponse {
                success: true,
                message: "OK".to_string(),
                data: Some(serde_json::json!({
                    "messages": messages,
                    "last_applied": node.get_status().await.last_applied,
                })),
            }
        }
    }
}

/// Status response for IPC
#[derive(Serialize, Deserialize)]
struct StatusResponse {
    node_id: String,
    role: String,
    current_term: u64,
    commit_index: u64,
    last_applied: u64,
    leader_id: Option<String>,
    peers: Vec<String>,
}

impl From<NodeStatus> for StatusResponse {
    fn from(s: NodeStatus) -> Self {
        Self {
            node_id: s.node_id,
            role: format!("{:?}", s.role),
            current_term: s.current_term,
            commit_index: s.commit_index,
            last_applied: s.last_applied,
            leader_id: s.leader_id,
            peers: s.peers,
        }
    }
}
