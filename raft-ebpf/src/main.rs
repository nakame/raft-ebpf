//! RAFT eBPF TC Classifier
//!
//! This eBPF program attaches to the TC (Traffic Control) hook to intercept
//! and monitor RAFT protocol packets. It:
//! - Detects packets destined for the RAFT port (5555)
//! - Tracks peer state in a BPF HashMap
//! - Sends packet events to userspace via a ring buffer

#![no_std]
#![no_main]

use aya_ebpf::{
    bindings::TC_ACT_PIPE,
    macros::{classifier, map},
    maps::{HashMap, RingBuf},
    programs::TcContext,
};
use aya_log_ebpf::info;
use network_types::{
    eth::{EthHdr, EtherType},
    ip::{IpProto, Ipv4Hdr},
    tcp::TcpHdr,
};
use raft_common::{PeerMetadata, RaftPacketEvent, RAFT_PORT};

/// Map storing peer metadata keyed by source IP address
#[map]
static PEER_STATE: HashMap<u32, PeerMetadata> = HashMap::with_max_entries(16, 0);

/// Ring buffer for sending packet events to userspace
#[map]
static RAFT_EVENTS: RingBuf = RingBuf::with_byte_size(256 * 1024, 0);

/// Statistics counters
#[map]
static STATS: HashMap<u32, u64> = HashMap::with_max_entries(8, 0);

const STAT_PACKETS_TOTAL: u32 = 0;
const STAT_RAFT_PACKETS: u32 = 1;
const STAT_ERRORS: u32 = 2;

/// TC classifier entry point for ingress traffic
#[classifier]
pub fn classify_raft(ctx: TcContext) -> i32 {
    match try_classify_raft(&ctx) {
        Ok(ret) => ret,
        Err(_) => {
            increment_stat(STAT_ERRORS);
            TC_ACT_PIPE
        }
    }
}

/// Main packet classification logic
fn try_classify_raft(ctx: &TcContext) -> Result<i32, ()> {
    increment_stat(STAT_PACKETS_TOTAL);

    // Parse Ethernet header
    let eth_hdr: EthHdr = ctx.load(0).map_err(|_| ())?;

    // Only process IPv4 packets
    if eth_hdr.ether_type != EtherType::Ipv4 {
        return Ok(TC_ACT_PIPE);
    }

    // Parse IPv4 header
    let ipv4_hdr: Ipv4Hdr = ctx.load(EthHdr::LEN).map_err(|_| ())?;

    // Only process TCP packets
    if ipv4_hdr.proto != IpProto::Tcp {
        return Ok(TC_ACT_PIPE);
    }

    // Calculate IPv4 header length (IHL field * 4)
    let ipv4_hdr_len = ((ipv4_hdr.version_ihl & 0x0f) as usize) * 4;

    // Parse TCP header
    let tcp_hdr: TcpHdr = ctx.load(EthHdr::LEN + ipv4_hdr_len).map_err(|_| ())?;

    let src_port = u16::from_be(tcp_hdr.source);
    let dst_port = u16::from_be(tcp_hdr.dest);

    // Check if this is RAFT traffic (either source or destination port)
    if dst_port != RAFT_PORT && src_port != RAFT_PORT {
        return Ok(TC_ACT_PIPE);
    }

    // This is RAFT traffic
    increment_stat(STAT_RAFT_PACKETS);

    let src_ip = u32::from_be(ipv4_hdr.src_addr);
    let dst_ip = u32::from_be(ipv4_hdr.dst_addr);
    let seq = u32::from_be(tcp_hdr.seq);
    let ack = u32::from_be(tcp_hdr.ack_seq);
    let flags = ((tcp_hdr.data_offset_reserved_flags >> 8) & 0x3f) as u8;

    // Get current timestamp
    let timestamp_ns = unsafe { aya_ebpf::helpers::bpf_ktime_get_ns() };

    // Update peer state
    let peer_meta = PeerMetadata {
        last_seen_ns: timestamp_ns,
        packet_count: 1,
        last_seq: seq,
        _padding: 0,
    };

    // Try to update existing entry or insert new one
    if let Some(existing) = unsafe { PEER_STATE.get_ptr_mut(&src_ip) } {
        unsafe {
            (*existing).last_seen_ns = timestamp_ns;
            (*existing).packet_count += 1;
            (*existing).last_seq = seq;
        }
    } else {
        let _ = PEER_STATE.insert(&src_ip, &peer_meta, 0);
    }

    // Send event to userspace via ring buffer
    if let Some(mut buf) = RAFT_EVENTS.reserve::<RaftPacketEvent>(0) {
        let event = RaftPacketEvent {
            src_ip,
            dst_ip,
            src_port,
            dst_port,
            timestamp_ns,
            seq,
            ack,
            flags,
            msg_type: 0, // Could parse payload to determine message type
            _padding: [0; 6],
        };
        buf.write(event);
        buf.submit(0);
    }

    info!(
        ctx,
        "RAFT packet: {}:{} -> {}:{}", src_ip, src_port, dst_ip, dst_port
    );

    // Pass packet through (don't drop)
    Ok(TC_ACT_PIPE)
}

/// Increment a statistics counter
fn increment_stat(key: u32) {
    if let Some(ptr) = unsafe { STATS.get_ptr_mut(&key) } {
        unsafe {
            *ptr += 1;
        }
    } else {
        let _ = STATS.insert(&key, &1u64, 0);
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
