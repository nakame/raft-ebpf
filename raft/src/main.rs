//! RAFT Consensus Daemon
//!
//! A distributed consensus system using the RAFT protocol with eBPF packet monitoring.

mod config;
#[cfg(all(target_os = "linux", feature = "ebpf"))]
mod ebpf;
mod ipc;
mod log;
mod network;
mod node;
mod state_machine;

use crate::config::Config;
use crate::ipc::IpcServer;
use crate::network::NetworkManager;
use crate::node::RaftNode;
use anyhow::Result;
use clap::Parser;
use raft_common::{MessageEnvelope, NodeRole};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

/// RAFT Consensus Daemon
#[derive(Parser, Debug)]
#[command(name = "raft")]
#[command(about = "A distributed RAFT consensus daemon with eBPF monitoring")]
struct Args {
    /// Path to configuration file
    #[arg(short, long)]
    config: PathBuf,

    /// Override log level
    #[arg(short, long)]
    log_level: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Load configuration
    let config = Config::load(&args.config)?;

    // Initialize logging
    let log_level = args
        .log_level
        .as_deref()
        .unwrap_or(&config.server.log_level);

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(log_level)
        }))
        .init();

    info!("Starting RAFT node: {}", config.server.node_id);
    info!("Listening on: {}", config.server.listen_addr);

    // Create channels for communication
    let (outgoing_tx, outgoing_rx) = mpsc::channel::<MessageEnvelope>(1024);

    // Create the RAFT node
    let node = RaftNode::new(config.clone(), outgoing_tx.clone()).await?;

    // Register peer addresses
    let mut network_manager =
        NetworkManager::new(node.clone(), config.server.listen_addr.clone(), outgoing_rx);

    for peer in &config.peers {
        network_manager
            .register_peer(peer.id.clone(), peer.address.clone())
            .await;
    }

    // Start IPC server
    let socket_path = PathBuf::from(format!("/tmp/raft-{}.sock", config.server.node_id));
    let ipc_server = IpcServer::new(node.clone(), socket_path);

    // Spawn IPC server
    let ipc_handle = tokio::spawn(async move {
        if let Err(e) = ipc_server.run().await {
            error!("IPC server error: {}", e);
        }
    });

    // Spawn network manager
    let network_handle = tokio::spawn(async move {
        if let Err(e) = network_manager.run().await {
            error!("Network manager error: {}", e);
        }
    });

    // Start election/heartbeat timers
    let node_for_election = node.clone();
    let node_for_heartbeat = node.clone();

    // Election timeout task
    let election_handle = tokio::spawn(async move {
        loop {
            let timeout = node_for_election.election_timeout();
            tokio::time::sleep(timeout).await;

            let role = node_for_election.role().await;
            if role == NodeRole::Follower || role == NodeRole::Candidate {
                let time_since_hb = node_for_election.time_since_heartbeat().await;
                if time_since_hb >= timeout {
                    info!("Election timeout, starting election");
                    if let Err(e) = node_for_election.start_election().await {
                        warn!("Failed to start election: {}", e);
                    }
                }
            }
        }
    });

    // Heartbeat task (for leader)
    let heartbeat_handle = tokio::spawn(async move {
        loop {
            let interval = node_for_heartbeat.heartbeat_interval();
            tokio::time::sleep(interval).await;

            let role = node_for_heartbeat.role().await;
            if role == NodeRole::Leader {
                if let Err(e) = node_for_heartbeat.send_heartbeats().await {
                    warn!("Failed to send heartbeats: {}", e);
                }
            }
        }
    });

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = ipc_handle => {
            error!("IPC server terminated unexpectedly");
        }
        _ = network_handle => {
            error!("Network manager terminated unexpectedly");
        }
    }

    info!("Shutting down RAFT node");
    Ok(())
}
