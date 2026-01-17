//! Configuration parsing and management

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Server configuration
    pub server: ServerConfig,
    /// Timing configuration
    #[serde(default)]
    pub timing: TimingConfig,
    /// Peer nodes
    #[serde(default)]
    pub peers: Vec<PeerConfig>,
    /// eBPF configuration
    #[serde(default)]
    pub ebpf: EbpfConfig,
}

/// Server-specific configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Unique node identifier
    pub node_id: String,
    /// Address to listen on (e.g., "0.0.0.0:5555")
    pub listen_addr: String,
    /// Directory for persistent data
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("/var/lib/raft")
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Timing configuration for RAFT protocol
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimingConfig {
    /// Heartbeat interval in milliseconds
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_ms: u64,
    /// Minimum election timeout in milliseconds
    #[serde(default = "default_election_timeout_min")]
    pub election_timeout_min_ms: u64,
    /// Maximum election timeout in milliseconds
    #[serde(default = "default_election_timeout_max")]
    pub election_timeout_max_ms: u64,
    /// RPC timeout in milliseconds
    #[serde(default = "default_rpc_timeout")]
    pub rpc_timeout_ms: u64,
}

fn default_heartbeat_interval() -> u64 {
    50
}
fn default_election_timeout_min() -> u64 {
    150
}
fn default_election_timeout_max() -> u64 {
    300
}
fn default_rpc_timeout() -> u64 {
    1000
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: default_heartbeat_interval(),
            election_timeout_min_ms: default_election_timeout_min(),
            election_timeout_max_ms: default_election_timeout_max(),
            rpc_timeout_ms: default_rpc_timeout(),
        }
    }
}

/// Peer node configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Peer's unique identifier
    pub id: String,
    /// Peer's network address
    pub address: String,
}

/// eBPF-specific configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EbpfConfig {
    /// Whether to enable eBPF packet monitoring
    #[serde(default = "default_ebpf_enabled")]
    pub enabled: bool,
    /// Network interface to attach to
    #[serde(default = "default_interface")]
    pub interface: String,
}

fn default_ebpf_enabled() -> bool {
    true
}

fn default_interface() -> String {
    "eth0".to_string()
}

impl Default for EbpfConfig {
    fn default() -> Self {
        Self {
            enabled: default_ebpf_enabled(),
            interface: default_interface(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Create a default configuration for testing
    pub fn default_for_testing(node_id: &str, port: u16) -> Self {
        Self {
            server: ServerConfig {
                node_id: node_id.to_string(),
                listen_addr: format!("127.0.0.1:{}", port),
                data_dir: PathBuf::from(format!("/tmp/raft-{}", node_id)),
                log_level: "debug".to_string(),
            },
            timing: TimingConfig::default(),
            peers: vec![],
            ebpf: EbpfConfig {
                enabled: false,
                interface: "lo".to_string(),
            },
        }
    }
}
