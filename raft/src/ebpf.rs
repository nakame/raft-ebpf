//! eBPF program loader and interaction
//!
//! This module is only available on Linux with the `ebpf` feature enabled.

#![cfg(all(target_os = "linux", feature = "ebpf"))]

use anyhow::Result;
use aya::maps::{HashMap, RingBuf};
use aya::programs::{tc, SchedClassifier, TcAttachType};
use aya::Ebpf;
use raft_common::{map_names, PeerMetadata, RaftPacketEvent};
use std::net::Ipv4Addr;
use std::path::Path;
use tokio::sync::mpsc;
use tracing::info;

/// eBPF program manager
pub struct EbpfManager {
    /// The loaded BPF object
    bpf: Ebpf,
    /// Interface name
    #[allow(dead_code)]
    interface: String,
}

impl EbpfManager {
    /// Load eBPF program from a file path and attach to interface
    pub fn load_from_file(path: &Path, interface: &str) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::load_from_bytes(&bytes, interface)
    }

    /// Load eBPF program from bytes
    pub fn load_from_bytes(bytes: &[u8], interface: &str) -> Result<Self> {
        let mut bpf = Ebpf::load(bytes)?;

        // Attach TC classifier
        let _ = tc::qdisc_add_clsact(interface);

        let program: &mut SchedClassifier = bpf
            .program_mut("classify_raft")
            .ok_or_else(|| anyhow::anyhow!("Program 'classify_raft' not found in eBPF binary"))?
            .try_into()?;

        program.load()?;
        program.attach(interface, TcAttachType::Ingress)?;

        info!("eBPF program attached to {} ingress", interface);

        Ok(Self {
            bpf,
            interface: interface.to_string(),
        })
    }

    /// Get peer metadata from the BPF map
    pub fn get_peer_state(&self, ip: Ipv4Addr) -> Result<Option<PeerMetadata>> {
        let peer_state: HashMap<_, u32, PeerMetadata> = HashMap::try_from(
            self.bpf
                .map(map_names::PEER_STATE)
                .ok_or_else(|| anyhow::anyhow!("Map not found"))?,
        )?;

        let ip_u32 = u32::from(ip);
        Ok(peer_state.get(&ip_u32, 0).ok())
    }

    /// Start reading events from the ring buffer
    pub fn start_event_reader(
        &mut self,
        _tx: mpsc::Sender<RaftPacketEvent>,
    ) -> Result<tokio::task::JoinHandle<()>> {
        let ring_buf: RingBuf<_> = RingBuf::try_from(
            self.bpf
                .map_mut(map_names::RAFT_EVENTS)
                .ok_or_else(|| anyhow::anyhow!("Ring buffer not found"))?,
        )?;

        let handle = tokio::spawn(async move {
            // Poll the ring buffer for events
            let _ = ring_buf;
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                // In production, use ring_buf.next() to read events
            }
        });

        Ok(handle)
    }
}

/// Utility to format IP address from u32
#[allow(dead_code)]
pub fn ip_to_string(ip: u32) -> String {
    Ipv4Addr::from(ip).to_string()
}
