//! RAFT CLI Tool
//!
//! Command-line interface for managing RAFT cluster nodes.

mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// RAFT Cluster Management CLI
#[derive(Parser, Debug)]
#[command(name = "raft-cli")]
#[command(about = "CLI tool for managing RAFT cluster nodes")]
#[command(version)]
struct Cli {
    /// Node ID to connect to
    #[arg(short, long, default_value = "node-0")]
    node: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Join a new node to the cluster
    Join {
        /// ID of the node to join
        node_id: String,
        /// Address of the node (ip:port)
        address: String,
    },
    /// Remove a node from the cluster
    Leave {
        /// ID of the node to remove
        node_id: String,
    },
    /// Get cluster status
    Status,
    /// Propose a key-value operation
    Propose {
        /// Operation: put or delete
        #[arg(value_parser = ["put", "delete", "get"])]
        operation: String,
        /// Key to operate on
        key: String,
        /// Value (required for put)
        value: Option<String>,
    },
    /// Send a message to the cluster (replicates via RAFT)
    Send {
        /// Message to send
        message: String,
    },
    /// Get recent messages from the cluster
    Messages {
        /// Maximum number of messages to retrieve
        #[arg(short, long, default_value = "10")]
        limit: u64,
    },
    /// Watch for new messages in real-time
    Watch {
        /// Poll interval in milliseconds
        #[arg(short, long, default_value = "500")]
        interval: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Join { node_id, address } => {
            commands::join(&cli.node, &node_id, &address).await?;
        }
        Commands::Leave { node_id } => {
            commands::leave(&cli.node, &node_id).await?;
        }
        Commands::Status => {
            commands::status(&cli.node).await?;
        }
        Commands::Propose {
            operation,
            key,
            value,
        } => {
            commands::propose(&cli.node, &operation, &key, value.as_deref()).await?;
        }
        Commands::Send { message } => {
            commands::send(&cli.node, &message).await?;
        }
        Commands::Messages { limit } => {
            commands::messages(&cli.node, limit).await?;
        }
        Commands::Watch { interval } => {
            commands::watch(&cli.node, interval).await?;
        }
    }

    Ok(())
}
