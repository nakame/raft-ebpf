#!/bin/bash
# RAFT Cluster Management Script
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RAFT_ROOT="$(dirname "$SCRIPT_DIR")"
NODES_DIR="$RAFT_ROOT/nodes"

usage() {
    echo "Usage: $0 <command>"
    echo ""
    echo "Commands:"
    echo "  create    - Create all 3 Lima VMs"
    echo "  start     - Start all VMs"
    echo "  stop      - Stop all VMs"
    echo "  delete    - Delete all VMs"
    echo "  status    - Show VM status"
    echo "  ips       - Show VM IPs and update configs"
    echo "  build     - Build RAFT in node-01"
    echo "  run       - Run RAFT on all nodes"
    echo "  shell <n> - Shell into node-0n (1,2,3)"
    echo ""
}

create_vms() {
    echo "Creating Lima VMs..."
    for i in 01 02 03; do
        echo "Creating raft-node-$i..."
        (cd "$NODES_DIR/node-$i" && limactl create --name="raft-node-$i" lima.yaml)
    done
    echo "Done. Run '$0 start' to start VMs."
}

start_vms() {
    echo "Starting Lima VMs..."
    for i in 01 02 03; do
        limactl start "raft-node-$i" &
    done
    wait
    echo "VMs started. Waiting for network..."
    sleep 5
    show_ips
}

stop_vms() {
    echo "Stopping Lima VMs..."
    for i in 01 02 03; do
        limactl stop "raft-node-$i" 2>/dev/null &
    done
    wait
    echo "Done."
}

delete_vms() {
    echo "Deleting Lima VMs..."
    for i in 01 02 03; do
        limactl delete "raft-node-$i" -f 2>/dev/null || true
    done
    echo "Done."
}

show_status() {
    limactl list | grep -E "NAME|raft-node"
}

show_ips() {
    echo ""
    echo "Node IPs:"
    echo "========="
    declare -A IPS
    for i in 01 02 03; do
        ip=$(limactl shell "raft-node-$i" -- hostname -I 2>/dev/null | awk '{print $1}')
        IPS[$i]=$ip
        echo "node-$i: $ip"
    done

    echo ""
    echo "Update raft.toml configs with these IPs:"
    echo "  node-01 peers: ${IPS[02]}:5555, ${IPS[03]}:5555"
    echo "  node-02 peers: ${IPS[01]}:5555, ${IPS[03]}:5555"
    echo "  node-03 peers: ${IPS[01]}:5555, ${IPS[02]}:5555"
}

build_raft() {
    echo "Building RAFT in node-01..."
    limactl shell raft-node-01 -- bash -c "
        cd ~/raft
        source ~/.cargo/env
        cargo build -p raft -p raft-cli --release
        cargo build -p raft-ebpf --target bpfel-unknown-none --release 2>/dev/null || echo 'eBPF build requires bpf-linker'
    "
    echo "Build complete."
}

run_nodes() {
    echo "Starting RAFT on all nodes..."
    for i in 01 02 03; do
        echo "Starting node-$i..."
        limactl shell "raft-node-$i" -- bash -c "
            cd ~/raft
            nohup ./target/release/raft --config nodes/node-$i/raft.toml > /tmp/raft-node-$i.log 2>&1 &
            echo \"node-$i started (PID: \$!)\"
        "
    done
    echo ""
    echo "Nodes started. Check logs:"
    echo "  limactl shell raft-node-01 -- tail -f /tmp/raft-node-01.log"
}

shell_into() {
    local n="$1"
    if [[ -z "$n" ]]; then
        echo "Usage: $0 shell <1|2|3>"
        exit 1
    fi
    limactl shell "raft-node-0$n"
}

case "${1:-}" in
    create) create_vms ;;
    start)  start_vms ;;
    stop)   stop_vms ;;
    delete) delete_vms ;;
    status) show_status ;;
    ips)    show_ips ;;
    build)  build_raft ;;
    run)    run_nodes ;;
    shell)  shell_into "$2" ;;
    *)      usage ;;
esac
