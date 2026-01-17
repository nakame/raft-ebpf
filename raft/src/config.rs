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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_timing_config_defaults() {
        let timing = TimingConfig::default();
        assert_eq!(timing.heartbeat_interval_ms, 50);
        assert_eq!(timing.election_timeout_min_ms, 150);
        assert_eq!(timing.election_timeout_max_ms, 300);
        assert_eq!(timing.rpc_timeout_ms, 1000);
    }

    #[test]
    fn test_ebpf_config_defaults() {
        let ebpf = EbpfConfig::default();
        assert!(ebpf.enabled);
        assert_eq!(ebpf.interface, "eth0");
    }

    #[test]
    fn test_default_for_testing() {
        let config = Config::default_for_testing("test-node", 6000);
        assert_eq!(config.server.node_id, "test-node");
        assert_eq!(config.server.listen_addr, "127.0.0.1:6000");
        assert_eq!(config.server.data_dir.to_str().unwrap(), "/tmp/raft-test-node");
        assert_eq!(config.server.log_level, "debug");
        assert!(!config.ebpf.enabled);
        assert!(config.peers.is_empty());
    }

    #[test]
    fn test_load_config_from_toml() {
        let toml_content = r#"
[server]
node_id = "node-01"
listen_addr = "0.0.0.0:5555"
data_dir = "/var/lib/raft/node-01"
log_level = "info"

[timing]
heartbeat_interval_ms = 100
election_timeout_min_ms = 200
election_timeout_max_ms = 400
rpc_timeout_ms = 2000

[ebpf]
enabled = false
interface = "ens192"

[[peers]]
id = "node-02"
address = "192.168.1.2:5555"

[[peers]]
id = "node-03"
address = "192.168.1.3:5555"
"#;

        let mut temp_file = NamedTempFile::new().expect("create temp file");
        temp_file.write_all(toml_content.as_bytes()).expect("write config");
        let path = temp_file.path();

        let config = Config::load(path).expect("load config");

        assert_eq!(config.server.node_id, "node-01");
        assert_eq!(config.server.listen_addr, "0.0.0.0:5555");
        assert_eq!(config.server.data_dir.to_str().unwrap(), "/var/lib/raft/node-01");
        assert_eq!(config.server.log_level, "info");

        assert_eq!(config.timing.heartbeat_interval_ms, 100);
        assert_eq!(config.timing.election_timeout_min_ms, 200);
        assert_eq!(config.timing.election_timeout_max_ms, 400);
        assert_eq!(config.timing.rpc_timeout_ms, 2000);

        assert!(!config.ebpf.enabled);
        assert_eq!(config.ebpf.interface, "ens192");

        assert_eq!(config.peers.len(), 2);
        assert_eq!(config.peers[0].id, "node-02");
        assert_eq!(config.peers[0].address, "192.168.1.2:5555");
        assert_eq!(config.peers[1].id, "node-03");
        assert_eq!(config.peers[1].address, "192.168.1.3:5555");
    }

    #[test]
    fn test_load_config_with_defaults() {
        let toml_content = r#"
[server]
node_id = "minimal-node"
listen_addr = "0.0.0.0:5555"
"#;

        let mut temp_file = NamedTempFile::new().expect("create temp file");
        temp_file.write_all(toml_content.as_bytes()).expect("write config");
        let path = temp_file.path();

        let config = Config::load(path).expect("load config");

        // Check defaults are applied
        assert_eq!(config.server.node_id, "minimal-node");
        assert_eq!(config.server.data_dir.to_str().unwrap(), "/var/lib/raft");
        assert_eq!(config.server.log_level, "info");

        // Timing defaults
        assert_eq!(config.timing.heartbeat_interval_ms, 50);
        assert_eq!(config.timing.election_timeout_min_ms, 150);
        assert_eq!(config.timing.election_timeout_max_ms, 300);

        // eBPF defaults
        assert!(config.ebpf.enabled);
        assert_eq!(config.ebpf.interface, "eth0");

        // No peers
        assert!(config.peers.is_empty());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default_for_testing("ser-test", 7000);
        let toml_str = toml::to_string(&config).expect("serialize");
        let parsed: Config = toml::from_str(&toml_str).expect("deserialize");

        assert_eq!(parsed.server.node_id, "ser-test");
        assert_eq!(parsed.server.listen_addr, "127.0.0.1:7000");
    }

    #[test]
    fn test_peer_config() {
        let peer = PeerConfig {
            id: "peer-01".to_string(),
            address: "10.0.0.1:5555".to_string(),
        };
        assert_eq!(peer.id, "peer-01");
        assert_eq!(peer.address, "10.0.0.1:5555");
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result = Config::load(std::path::Path::new("/nonexistent/path/config.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_toml() {
        let invalid_toml = "this is not valid toml {{{";

        let mut temp_file = NamedTempFile::new().expect("create temp file");
        temp_file.write_all(invalid_toml.as_bytes()).expect("write");
        let path = temp_file.path();

        let result = Config::load(path);
        assert!(result.is_err());
    }
}
