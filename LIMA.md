# Lima VM Setup for RAFT Cluster

This document describes how to set up Lima VMs with vmnet networking for testing the RAFT cluster.

## Prerequisites

- macOS with Apple Silicon (aarch64) or Intel (x86_64)
- Homebrew installed
- Lima installed (`brew install lima`)

## Quick Start (All sudo commands)

Run these commands in order to set up socket_vmnet and Lima:

```bash
# 1. Install socket_vmnet
brew install socket_vmnet

# 2. Create root-owned directory and copy binary
sudo mkdir -p /opt/socket_vmnet/bin
sudo cp /opt/homebrew/Cellar/socket_vmnet/$(brew info socket_vmnet --json | jq -r '.[0].versions.stable')/bin/socket_vmnet /opt/socket_vmnet/bin/
sudo chown -R root:admin /opt/socket_vmnet
sudo chmod 755 /opt/socket_vmnet/bin/socket_vmnet

# 3. Start socket_vmnet service
sudo brew services start socket_vmnet

# 4. Generate and install sudoers file
limactl sudoers > /tmp/etc_sudoers.d_lima
sudo install -o root /tmp/etc_sudoers.d_lima /private/etc/sudoers.d/lima
```

## socket_vmnet Setup

Lima requires `socket_vmnet` for shared networking between VMs. This allows VMs to communicate with each other on a virtual network (192.168.105.0/24).

### 1. Install socket_vmnet

```bash
brew install socket_vmnet
```

### 2. Set up root-owned binary location

Lima requires the socket_vmnet binary to be in a root-owned path (security requirement). Copy it to `/opt/socket_vmnet`:

```bash
sudo mkdir -p /opt/socket_vmnet/bin
sudo cp /opt/homebrew/Cellar/socket_vmnet/$(brew info socket_vmnet --json | jq -r '.[0].versions.stable')/bin/socket_vmnet /opt/socket_vmnet/bin/
sudo chown -R root:admin /opt/socket_vmnet
sudo chmod 755 /opt/socket_vmnet/bin/socket_vmnet
```

### 3. Start the socket_vmnet service

```bash
sudo brew services start socket_vmnet
```

Expected output:
```
==> Successfully started `socket_vmnet` (label: homebrew.mxcl.socket_vmnet)
```

### 4. Configure Lima networks

Create or update `~/.lima/_config/networks.yaml`:

```yaml
paths:
  socketVMNet: "/opt/socket_vmnet/bin/socket_vmnet"
  varRun: /private/var/run/lima
  sudoers: /private/etc/sudoers.d/lima

group: everyone

networks:
  shared:
    mode: shared
    gateway: 192.168.105.1
    dhcpEnd: 192.168.105.254
    netmask: 255.255.255.0
  bridged:
    mode: bridged
    interface: en0
  host:
    mode: host
    gateway: 192.168.106.1
    dhcpEnd: 192.168.106.254
    netmask: 255.255.255.0
```

### 5. Set up sudoers (required for vmnet)

Generate and install the sudoers configuration:

```bash
limactl sudoers > /tmp/etc_sudoers.d_lima
sudo install -o root /tmp/etc_sudoers.d_lima /private/etc/sudoers.d/lima
```

This allows Lima to start the socket_vmnet daemon without prompting for a password each time.

## Upgrading socket_vmnet

When upgrading socket_vmnet via Homebrew, you must re-copy the binary:

```bash
brew upgrade socket_vmnet
sudo cp /opt/homebrew/Cellar/socket_vmnet/$(brew info socket_vmnet --json | jq -r '.[0].versions.stable')/bin/socket_vmnet /opt/socket_vmnet/bin/
sudo chown root:admin /opt/socket_vmnet/bin/socket_vmnet
sudo chmod 755 /opt/socket_vmnet/bin/socket_vmnet
sudo brew services restart socket_vmnet
```

## RAFT Cluster VMs

The cluster consists of 3 Lima VMs on the shared network:

| Node | VM Name | IP (DHCP) | RAFT Port |
|------|---------|-----------|-----------|
| node-01 | raft-node-01 | 192.168.105.x | 5555 |
| node-02 | raft-node-02 | 192.168.105.x | 5555 |
| node-03 | raft-node-03 | 192.168.105.x | 5555 |

### Create and Start VMs

```bash
# Create VMs
cd ~/raft/nodes/node-01 && limactl create --name=raft-node-01 lima.yaml
cd ~/raft/nodes/node-02 && limactl create --name=raft-node-02 lima.yaml
cd ~/raft/nodes/node-03 && limactl create --name=raft-node-03 lima.yaml

# Start VMs (one at a time)
limactl start raft-node-01
limactl start raft-node-02
limactl start raft-node-03
```

Sample startup output:
```
time="2026-01-17T10:16:04+08:00" level=info msg="Using the existing instance \"raft-node-01\""
time="2026-01-17T10:16:04+08:00" level=info msg="Starting socket_vmnet daemon for \"shared\" network"
time="2026-01-17T10:16:04+08:00" level=info msg="Starting the instance \"raft-node-01\" with internal VM driver \"vz\""
time="2026-01-17T10:16:04+08:00" level=info msg="[hostagent] hostagent socket created at /Users/jase/.lima/raft-node-01/ha.sock"
time="2026-01-17T10:16:04+08:00" level=info msg="[hostagent] Starting VZ"
time="2026-01-17T10:16:04+08:00" level=info msg="[hostagent] [VZ] - vm state change: running"
...
time="2026-01-17T10:16:25+08:00" level=info msg="[hostagent] Not forwarding UDP 192.168.105.11:68"
...
time="2026-01-17T10:17:05+08:00" level=info msg="READY. Run `limactl shell raft-node-01` to open the shell."
RAFT node-01 ready!
Shell: limactl shell raft-node-01
```

The `192.168.105.x:68` line shows the VM's IP address on the shared network.

### Get VM IPs

After starting, get the assigned IPs:

```bash
limactl shell raft-node-01 -- hostname -I
limactl shell raft-node-02 -- hostname -I
limactl shell raft-node-03 -- hostname -I
```

Typical output (IPs are assigned by DHCP):
```
192.168.105.11 192.168.5.15   # node-01
192.168.105.12 192.168.5.15   # node-02
192.168.105.13 192.168.5.15   # node-03
```

The `192.168.105.x` addresses are on the shared vmnet network. Update `nodes/node-*/raft.toml` with these IPs.

### Shell into VMs

```bash
limactl shell raft-node-01
limactl shell raft-node-02
limactl shell raft-node-03
```

### Stop/Delete VMs

```bash
# Stop
limactl stop raft-node-01 raft-node-02 raft-node-03

# Delete
limactl delete raft-node-01 raft-node-02 raft-node-03
```

## Troubleshooting

### "socketVMNet is not owned by root"

The entire path to socket_vmnet must be root-owned. Use `/opt/socket_vmnet/bin/socket_vmnet` as shown above.

### "file is a symlink"

Lima doesn't allow symlinks in the socketVMNet path. Use the actual path, not `/opt/homebrew/opt/socket_vmnet`.

### VMs can't communicate

1. Check socket_vmnet service is running: `sudo brew services list | grep socket`
2. Verify VMs are on shared network: `limactl shell <vm> -- ip addr show`
3. Check firewall isn't blocking: `sudo pfctl -s rules`

### Provisioning script warnings

The warning "provisioning scripts should not reference the LIMA_CIDATA variables" is harmless and can be ignored.
