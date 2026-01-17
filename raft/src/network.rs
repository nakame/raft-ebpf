//! TCP networking for RAFT RPC communication

use crate::node::RaftNode;
use anyhow::Result;
use bytes::{Buf, BufMut, BytesMut};
use raft_common::MessageEnvelope;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, info, warn};

/// Message framing: 4-byte length prefix + JSON payload
const MAX_MESSAGE_SIZE: usize = 1024 * 1024; // 1MB

/// Network manager for RAFT communication
pub struct NetworkManager {
    /// Our node reference
    node: Arc<RaftNode>,
    /// Address to listen on
    listen_addr: String,
    /// Outgoing message receiver
    outgoing_rx: mpsc::Receiver<MessageEnvelope>,
    /// Active peer connections
    connections: Arc<Mutex<HashMap<String, Arc<Mutex<TcpStream>>>>>,
    /// Peer addresses
    peer_addresses: Arc<Mutex<HashMap<String, String>>>,
}

impl NetworkManager {
    /// Create a new network manager
    pub fn new(
        node: Arc<RaftNode>,
        listen_addr: String,
        outgoing_rx: mpsc::Receiver<MessageEnvelope>,
    ) -> Self {
        Self {
            node,
            listen_addr,
            outgoing_rx,
            connections: Arc::new(Mutex::new(HashMap::new())),
            peer_addresses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a peer's address
    pub async fn register_peer(&self, peer_id: String, address: String) {
        self.peer_addresses.lock().await.insert(peer_id, address);
    }

    /// Run the network manager
    pub async fn run(mut self) -> Result<()> {
        // Start TCP listener
        let listener = TcpListener::bind(&self.listen_addr).await?;
        info!("RAFT node listening on {}", self.listen_addr);

        // Spawn listener task
        let node = self.node.clone();
        let connections = self.connections.clone();
        let peer_addresses = self.peer_addresses.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        debug!("Accepted connection from {}", addr);
                        let node = node.clone();
                        let connections = connections.clone();
                        let peer_addresses = peer_addresses.clone();

                        tokio::spawn(async move {
                            if let Err(e) =
                                handle_connection(stream, node, connections, peer_addresses).await
                            {
                                warn!("Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
        });

        // Process outgoing messages
        while let Some(envelope) = self.outgoing_rx.recv().await {
            let to = envelope.to.as_str().to_string();
            if let Err(e) = self.send_message(&to, &envelope).await {
                warn!("Failed to send message to {}: {}", to, e);
            }
        }

        Ok(())
    }

    /// Send a message to a peer
    async fn send_message(&self, peer_id: &str, envelope: &MessageEnvelope) -> Result<()> {
        // Get or create connection
        let stream = {
            let connections = self.connections.lock().await;
            connections.get(peer_id).cloned()
        };

        let stream = match stream {
            Some(s) => s,
            None => {
                // Create new connection
                let addr = {
                    let addresses = self.peer_addresses.lock().await;
                    addresses.get(peer_id).cloned()
                };

                let addr = match addr {
                    Some(a) => a,
                    None => {
                        warn!("No address for peer {}", peer_id);
                        return Ok(());
                    }
                };

                let stream = TcpStream::connect(&addr).await?;
                let stream = Arc::new(Mutex::new(stream));
                self.connections
                    .lock()
                    .await
                    .insert(peer_id.to_string(), stream.clone());
                stream
            }
        };

        // Serialize and send message
        let payload = serde_json::to_vec(envelope)?;
        let mut buf = BytesMut::with_capacity(4 + payload.len());
        buf.put_u32(payload.len() as u32);
        buf.put_slice(&payload);

        let mut stream = stream.lock().await;
        stream.write_all(&buf).await?;
        stream.flush().await?;

        Ok(())
    }
}

/// Handle an incoming connection
async fn handle_connection(
    mut stream: TcpStream,
    node: Arc<RaftNode>,
    connections: Arc<Mutex<HashMap<String, Arc<Mutex<TcpStream>>>>>,
    _peer_addresses: Arc<Mutex<HashMap<String, String>>>,
) -> Result<()> {
    let mut buf = BytesMut::with_capacity(4096);

    loop {
        // Read more data
        let n = stream.read_buf(&mut buf).await?;
        if n == 0 {
            debug!("Connection closed");
            break;
        }

        // Process complete messages
        while buf.len() >= 4 {
            let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;

            if len > MAX_MESSAGE_SIZE {
                error!("Message too large: {} bytes", len);
                return Ok(());
            }

            if buf.len() < 4 + len {
                // Wait for more data
                break;
            }

            // Extract message
            buf.advance(4);
            let payload = buf.split_to(len);

            // Parse and handle message
            match serde_json::from_slice::<MessageEnvelope>(&payload) {
                Ok(envelope) => {
                    let from = envelope.from.as_str().to_string();
                    debug!("Received message from {}: {:?}", from, envelope.message);

                    // Store connection for this peer if not already stored
                    // Note: This is simplified; in production you'd want bidirectional connection management

                    if let Err(e) = node.handle_message(envelope).await {
                        warn!("Error handling message: {}", e);
                    }
                }
                Err(e) => {
                    warn!("Failed to parse message: {}", e);
                }
            }
        }
    }

    Ok(())
}

/// Encode a message for transmission
pub fn encode_message(envelope: &MessageEnvelope) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(envelope)?;
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    buf.extend_from_slice(&payload);
    Ok(buf)
}

/// Decode a message from bytes
pub fn decode_message(data: &[u8]) -> Result<MessageEnvelope> {
    if data.len() < 4 {
        anyhow::bail!("Message too short");
    }
    let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 4 + len {
        anyhow::bail!("Incomplete message");
    }
    let envelope: MessageEnvelope = serde_json::from_slice(&data[4..4 + len])?;
    Ok(envelope)
}
