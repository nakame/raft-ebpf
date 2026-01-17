//! IPC client for communicating with the RAFT daemon

use super::{IpcRequest, IpcResponse};
use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// IPC client for communicating with a RAFT node
pub struct IpcClient {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl IpcClient {
    /// Connect to a RAFT node via Unix socket
    pub async fn connect(node_id: &str) -> Result<Self> {
        let socket_path = format!("/tmp/raft-{}.sock", node_id);
        let stream = UnixStream::connect(&socket_path).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to connect to node '{}' at {}: {}. Is the node running?",
                node_id,
                socket_path,
                e
            )
        })?;

        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
        })
    }

    /// Send a request and receive a response
    pub async fn send_request(&mut self, request: &IpcRequest) -> Result<IpcResponse> {
        // Send request
        let request_str = serde_json::to_string(request)? + "\n";
        self.writer.write_all(request_str.as_bytes()).await?;
        self.writer.flush().await?;

        // Read response
        let mut line = String::new();
        self.reader.read_line(&mut line).await?;

        if line.is_empty() {
            anyhow::bail!("Connection closed by server");
        }

        let response: IpcResponse = serde_json::from_str(&line)?;
        Ok(response)
    }
}
