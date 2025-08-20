# Redfire Gateway Deployment Guide

This guide covers the installation, configuration, and deployment of the Redfire Gateway in production environments.

## Overview

The Redfire Gateway is a carrier-grade telecommunications system that bridges legacy TDM infrastructure with modern SIP/VoIP networks. It provides comprehensive TDMoE (TDM over Ethernet) to SIP gateway functionality with advanced features for enterprise and service provider deployments.

## System Requirements

### Minimum Hardware Requirements

- **CPU**: 2+ cores, x86_64 architecture
- **Memory**: 4GB RAM minimum, 8GB recommended
- **Storage**: 20GB free space minimum, 100GB recommended
- **Network**: Gigabit Ethernet interfaces
- **Operating System**: Debian 11+ or Ubuntu 20.04+

### Recommended Hardware for Production

- **CPU**: 4+ cores, Intel Xeon or AMD EPYC
- **Memory**: 16GB+ RAM
- **Storage**: SSD storage, 500GB+
- **Network**: Multiple Gigabit Ethernet interfaces
- **Redundancy**: Dual power supplies, RAID storage

### Network Requirements

- **TDMoE Interface**: Dedicated Ethernet interface for TDM traffic
- **SIP Interface**: Network access for SIP signaling (UDP/TCP port 5060)
- **RTP Interface**: Port range for media streams (default: 10000-20000)
- **Management Interface**: HTTPS (443), SSH (22), SNMP (161)

## Installation Methods

### Method 1: Debian Package Installation (Recommended)

```bash
# Download the latest package
wget https://releases.redfire.tel/redfire-gateway_1.0.0-1_amd64.deb

# Install the package
sudo dpkg -i redfire-gateway_1.0.0-1_amd64.deb

# Fix any dependency issues
sudo apt-get install -f

# Verify installation
sudo systemctl status redfire-gateway
```

### Method 2: From Source

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Clone and build
git clone https://github.com/redfiretel/redfire-gateway.git
cd redfire-gateway
cargo build --release

# Install manually
sudo cp target/release/redfire-gateway /usr/bin/
sudo cp target/release/redfire-cli /usr/bin/
sudo cp target/release/redfire-diag /usr/bin/
sudo cp target/release/interface-test /usr/bin/
sudo cp target/release/timing-manager /usr/bin/

# Create user and directories
sudo useradd --system --home /var/lib/redfire-gateway redfire
sudo mkdir -p /etc/redfire-gateway /var/log/redfire-gateway
sudo chown redfire:redfire /var/lib/redfire-gateway /var/log/redfire-gateway
```

### Method 3: Docker Deployment

```bash
# Pull the official image
docker pull redfire/gateway:latest

# Run with docker-compose (see docker-compose.yml)
docker-compose up -d
```

## Configuration

### Main Configuration File

The primary configuration is located at `/etc/redfire-gateway/gateway.toml`:

```toml
[general]
name = "Redfire Gateway"
location = "Primary Site"
contact = "ops@company.com"
timezone = "UTC"

[tdmoe]
interface = "eth1"  # Dedicated TDMoE interface
channels = 30       # E1: 30, T1: 24
protocol = "E1"     # E1 or T1
framing = "CRC4"    # E1: CRC4, T1: ESF
line_coding = "HDB3" # E1: HDB3, T1: B8ZS

[sip]
listen_addr = "0.0.0.0:5060"
domain = "gateway.company.com"
realm = "company"
transport = "UDP"   # UDP, TCP, or TLS
registration_server = "sip.company.com"

[rtp]
port_range = "10000-20000"
dscp_marking = 46   # EF for voice traffic
jitter_buffer = 50  # milliseconds

[timing]
enable_internal_clock = true
enable_gps = true
enable_ntp = true
gps_device = "/dev/ttyUSB0"
ntp_servers = ["pool.ntp.org", "time.google.com"]
clock_selection_algorithm = "HighestStratum"

[logging]
level = "info"      # trace, debug, info, warn, error
file = "/var/log/redfire-gateway/gateway.log"
max_size = 10485760 # 10MB
max_files = 5
format = "json"     # json or text

[snmp]
enabled = true
community = "public"
port = 161
location = "Data Center A"
contact = "noc@company.com"
```

### Security Configuration

```toml
[security]
enable_tls = true
certificate_file = "/etc/redfire-gateway/ssl/server.crt"
private_key_file = "/etc/redfire-gateway/ssl/server.key"
ca_certificate_file = "/etc/redfire-gateway/ssl/ca.crt"

[authentication]
sip_authentication = true
digest_realm = "secure-gateway"
user_database = "/etc/redfire-gateway/users.db"
```

### High Availability Configuration

```toml
[ha]
enabled = true
node_id = "gateway-01"
cluster_interface = "eth2"
peer_nodes = ["192.168.1.11", "192.168.1.12"]
heartbeat_interval = 1000  # milliseconds
failover_timeout = 5000    # milliseconds

[backup]
enabled = true
backup_server = "backup.company.com"
backup_interval = 3600     # seconds
retention_days = 30
```

## Service Management

### SystemD Commands

```bash
# Start services
sudo systemctl start redfire-gateway
sudo systemctl start redfire-timing

# Stop services
sudo systemctl stop redfire-gateway
sudo systemctl stop redfire-timing

# Restart services
sudo systemctl restart redfire-gateway

# Enable auto-start
sudo systemctl enable redfire-gateway
sudo systemctl enable redfire-timing

# Check status
sudo systemctl status redfire-gateway
sudo systemctl status redfire-timing

# View logs
sudo journalctl -u redfire-gateway -f
sudo journalctl -u redfire-timing -f
```

### Management Commands

```bash
# Gateway status
redfire-cli status

# System diagnostics
redfire-diag system

# Interface testing
interface-test loopback --span 1 --duration 30

# Timing management
timing-manager status --detailed

# Performance monitoring
redfire-cli performance

# Configuration validation
redfire-cli config validate
```

## Network Configuration

### Interface Setup

```bash
# Configure TDMoE interface (Debian/Ubuntu)
sudo vi /etc/netplan/01-redfire.yaml
```

```yaml
network:
  version: 2
  ethernets:
    eth1:  # TDMoE interface
      dhcp4: false
      addresses:
        - 192.168.100.10/24
      mtu: 1500
      
    eth2:  # Management interface
      dhcp4: true
      
    eth3:  # HA cluster interface
      dhcp4: false
      addresses:
        - 192.168.200.10/24
```

### Firewall Configuration

```bash
# UFW (Ubuntu Firewall)
sudo ufw allow 5060/udp    # SIP signaling
sudo ufw allow 5060/tcp    # SIP over TCP
sudo ufw allow 5061/tcp    # SIP over TLS
sudo ufw allow 10000:20000/udp  # RTP media
sudo ufw allow 161/udp     # SNMP
sudo ufw allow 22/tcp      # SSH management

# iptables rules
sudo iptables -A INPUT -p udp --dport 5060 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 5060 -j ACCEPT
sudo iptables -A INPUT -p udp --dport 10000:20000 -j ACCEPT
```

## Monitoring and Maintenance

### Health Checks

```bash
#!/bin/bash
# Health check script

# Check service status
systemctl is-active redfire-gateway >/dev/null || exit 1

# Check SIP registration
redfire-cli sip status | grep -q "Registered" || exit 1

# Check TDM interface
redfire-diag tdm status | grep -q "Up" || exit 1

# Check timing synchronization
timing-manager status | grep -q "Synchronized" || exit 1

echo "All systems operational"
```

### Log Monitoring

```bash
# Monitor critical events
tail -f /var/log/redfire-gateway/gateway.log | grep -E "(ERROR|CRITICAL)"

# Monitor call statistics
redfire-cli stats calls --live

# Monitor interface status
watch -n 5 "redfire-diag interfaces"
```

### Performance Monitoring

```bash
# System resources
redfire-cli system resources

# Call quality metrics
redfire-cli quality --interval 60

# Network statistics
redfire-cli network stats

# Timing accuracy
timing-manager stats
```

## Troubleshooting

### Common Issues

#### 1. Service Won't Start

```bash
# Check configuration
redfire-cli config validate

# Check permissions
sudo chown -R redfire:redfire /var/lib/redfire-gateway
sudo chown -R redfire:redfire /var/log/redfire-gateway

# Check system resources
df -h
free -m
```

#### 2. No SIP Registration

```bash
# Check SIP configuration
redfire-cli sip test

# Check network connectivity
ping sip.provider.com

# Check firewall
sudo ufw status
sudo iptables -L
```

#### 3. Poor Call Quality

```bash
# Check RTP statistics
redfire-cli rtp stats

# Check network latency
redfire-diag network latency

# Check timing synchronization
timing-manager status --detailed
```

#### 4. TDM Interface Issues

```bash
# Check interface status
redfire-diag tdm status

# Test loopback
interface-test loopback --span 1

# Check physical connections
redfire-diag hardware
```

### Diagnostic Tools

```bash
# Generate support bundle
redfire-diag support-bundle --output /tmp/gateway-support.tar.gz

# Run comprehensive tests
interface-test suite "1,2,3" --loopback --cross-port --duration 60

# Protocol analysis
redfire-diag protocols --capture 60

# Performance analysis
redfire-diag performance --duration 300
```

## Security Hardening

### System Security

```bash
# Disable unused services
sudo systemctl disable bluetooth
sudo systemctl disable cups

# Configure fail2ban
sudo apt install fail2ban
sudo cp /etc/fail2ban/jail.conf /etc/fail2ban/jail.local

# Set up firewall
sudo ufw enable
sudo ufw default deny incoming
sudo ufw default allow outgoing
```

### Application Security

```bash
# Generate SSL certificates
sudo openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
    -keyout /etc/redfire-gateway/ssl/server.key \
    -out /etc/redfire-gateway/ssl/server.crt

# Set secure permissions
sudo chmod 600 /etc/redfire-gateway/ssl/server.key
sudo chmod 644 /etc/redfire-gateway/ssl/server.crt
sudo chown root:redfire /etc/redfire-gateway/ssl/*
```

## Backup and Recovery

### Configuration Backup

```bash
#!/bin/bash
# Backup script

DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="/backup/redfire-gateway"

mkdir -p "$BACKUP_DIR"

# Backup configuration
tar -czf "$BACKUP_DIR/config_$DATE.tar.gz" /etc/redfire-gateway/

# Backup user data
tar -czf "$BACKUP_DIR/data_$DATE.tar.gz" /var/lib/redfire-gateway/

# Backup logs (last 7 days)
find /var/log/redfire-gateway/ -name "*.log" -mtime -7 \
    -exec tar -czf "$BACKUP_DIR/logs_$DATE.tar.gz" {} +

echo "Backup completed: $BACKUP_DIR"
```

### Recovery Procedure

```bash
# Stop services
sudo systemctl stop redfire-gateway redfire-timing

# Restore configuration
sudo tar -xzf config_backup.tar.gz -C /

# Restore data
sudo tar -xzf data_backup.tar.gz -C /

# Fix permissions
sudo chown -R redfire:redfire /var/lib/redfire-gateway

# Start services
sudo systemctl start redfire-gateway redfire-timing
```

## Scaling and Load Balancing

### Load Balancer Configuration (nginx)

```nginx
upstream redfire_gateways {
    server 192.168.1.10:5060 weight=1 max_fails=3 fail_timeout=30s;
    server 192.168.1.11:5060 weight=1 max_fails=3 fail_timeout=30s;
    server 192.168.1.12:5060 weight=1 max_fails=3 fail_timeout=30s;
}

server {
    listen 5060 udp;
    proxy_pass redfire_gateways;
    proxy_timeout 1s;
    proxy_responses 1;
}
```

### Database Clustering (if applicable)

```toml
[database]
type = "postgresql"
cluster_mode = true
primary_host = "db1.company.com"
replica_hosts = ["db2.company.com", "db3.company.com"]
connection_pool_size = 20
```

## Support and Maintenance

### Regular Maintenance Tasks

```bash
# Weekly tasks
sudo logrotate /etc/logrotate.d/redfire-gateway
sudo systemctl restart redfire-gateway  # if needed

# Monthly tasks
redfire-cli maintenance --optimize-database
redfire-cli maintenance --cleanup-logs --days 30

# Quarterly tasks
redfire-cli maintenance --full-system-check
interface-test suite "1,2,3" --comprehensive
```

### Getting Support

- **Documentation**: https://docs.redfire.tel/
- **Community Forum**: https://community.redfire.tel/
- **Enterprise Support**: support@redfire.tel
- **Emergency Support**: +1-800-REDFIRE

### Version Management

```bash
# Check current version
redfire-cli version

# Check for updates
redfire-cli update check

# Backup before updating
/usr/local/bin/backup-redfire.sh

# Update (package manager)
sudo apt update && sudo apt upgrade redfire-gateway
```

## Appendices

### A. Port Reference

| Service | Port | Protocol | Description |
|---------|------|----------|-------------|
| SIP | 5060 | UDP/TCP | SIP signaling |
| SIP TLS | 5061 | TCP | Secure SIP |
| RTP | 10000-20000 | UDP | Media streams |
| SNMP | 161 | UDP | Network management |
| SSH | 22 | TCP | Remote access |
| HTTPS | 443 | TCP | Web management |

### B. Configuration Templates

Additional configuration templates are available in:
- `/usr/share/redfire-gateway/config/`
- `/usr/share/doc/redfire-gateway/examples/`

### C. Performance Tuning

See the Performance Tuning Guide at:
https://docs.redfire.tel/performance-tuning/