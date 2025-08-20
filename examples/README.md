# Redfire Gateway Configuration Examples

This directory contains example configurations for different deployment scenarios. Choose the configuration that best matches your use case as a starting point.

## 📋 Available Examples

### `basic-gateway.toml`
**Use case**: Development, testing, small deployments
- Minimal configuration with essential settings
- Single E1 interface
- Basic SIP configuration
- 100 concurrent calls
- Local file logging
- No clustering or advanced features

**Quick start**:
```bash
cp examples/basic-gateway.toml /etc/redfire/gateway.toml
redfire-gateway --config /etc/redfire/gateway.toml
```

### `production-gateway.toml`
**Use case**: Production deployments, enterprise environments
- Full feature set enabled
- Multiple trunk types (E1/T1)
- TLS security for SIP
- Comprehensive routing rules
- SNMP monitoring
- Performance thresholds
- 5000 concurrent calls
- Emergency services routing
- Mobile network integration

**Features**:
- High-capacity configuration (5000 calls)
- Security-focused (TLS, authentication)
- Comprehensive monitoring and alerting
- Multiple codec support including HD voice
- Advanced routing with number translation

### `mobile-gateway.toml`
**Use case**: Mobile network operators, VoLTE/VoWiFi integration
- Specialized for mobile networks (3G, 4G, VoLTE, VoWiFi)
- AMR, AMR-WB, and EVS codec support
- Mobile-specific QoS configuration
- Enhanced emergency services
- Optimized for mobile call patterns
- 10000 concurrent calls

**Mobile features**:
- Full AMR codec family support
- EVS (Enhanced Voice Services) codec
- Mobile QoS parameters
- Emergency location services
- Optimized jitter buffers for mobile networks

### `clustering-gateway.toml`
**Use case**: High availability deployments, carrier-grade environments
- Multi-node clustering configuration
- Shared state management with Redis
- Anycast IP address support
- Automatic failover capabilities
- NFAS span redundancy
- Distributed transaction synchronization

**HA features**:
- Multiple node support
- Shared routing tables
- Automatic failover
- State synchronization
- Load balancing

## 🔧 Configuration Guidelines

### Step 1: Choose Base Configuration
Select the example that closest matches your deployment:
- **Development/Testing**: `basic-gateway.toml`
- **Production Single Node**: `production-gateway.toml`
- **Mobile Networks**: `mobile-gateway.toml`
- **High Availability**: `clustering-gateway.toml`

### Step 2: Customize for Your Environment

#### Network Interfaces
```toml
[tdmoe]
interface = "eth0"  # Change to your TDM interface

[e1]
interface = "span1"  # Change to your E1 span
```

#### SIP Configuration
```toml
[sip]
listen_port = 5060
domain = "your-domain.com"  # Your SIP domain
transport = "udp"           # udp, tcp, or tls
```

#### Capacity Planning
```toml
[general]
max_calls = 1000           # Adjust based on capacity needs

[rtp]
port_range = { min = 10000, max = 20000 }  # Ensure sufficient RTP ports
```

### Step 3: Beta Integration Considerations

⚠️ **Important**: This beta release requires external library integration:

#### SIP Integration
The SIP functionality uses stub implementations. You must:
1. Choose a compatible Rust SIP library
2. Implement the `SipHandler` trait
3. Replace the stub implementation
4. See [INTEGRATION.md](../INTEGRATION.md) for details

#### Transcoding Integration
The transcoding functionality uses stub implementations. You must:
1. Choose a transcoding library (CPU or GPU-accelerated)
2. Implement the `TranscodingService` trait
3. Replace the stub implementation
4. Update configuration: `enable_codec_transcoding = true`

### Step 4: Security Configuration

#### TLS Configuration (Production)
```toml
[sip]
transport = "tls"

# Add TLS certificate configuration when SIP library is integrated
```

#### SNMP Security
```toml
[snmp]
enabled = true
version = "v3"              # Use SNMPv3 for security
community = "secure_string" # Change default community
```

### Step 5: Monitoring Setup

#### Performance Monitoring
```toml
[performance]
enabled = true
interval = 5000            # 5 second intervals
history_size = 720         # 1 hour of history

[performance.thresholds.cpu]
warning = 80.0
critical = 95.0
```

#### Logging Configuration
```toml
[logging]
level = "info"                              # debug, info, warn, error
file = "/var/log/redfire-gateway.log"
format = "json"                            # json or compact
max_size = 104857600                       # 100MB
max_files = 10
```

## 🚀 Deployment Examples

### Single Node Development
```bash
# Copy basic configuration
cp examples/basic-gateway.toml /etc/redfire/gateway.toml

# Edit for your environment
sudo nano /etc/redfire/gateway.toml

# Start gateway
redfire-gateway --config /etc/redfire/gateway.toml
```

### Production Deployment
```bash
# Copy production configuration
cp examples/production-gateway.toml /etc/redfire/gateway.toml

# Secure the configuration file
sudo chown root:redfire /etc/redfire/gateway.toml
sudo chmod 640 /etc/redfire/gateway.toml

# Edit for your environment
sudo nano /etc/redfire/gateway.toml

# Validate configuration
redfire-gateway --config /etc/redfire/gateway.toml --validate

# Start as service
sudo systemctl start redfire-gateway
sudo systemctl enable redfire-gateway
```

### High Availability Cluster
```bash
# Node 1
cp examples/clustering-gateway.toml /etc/redfire/gateway-node1.toml
# Edit: node_id = "node-01"

# Node 2
cp examples/clustering-gateway.toml /etc/redfire/gateway-node2.toml
# Edit: node_id = "node-02", logging path

# Node 3
cp examples/clustering-gateway.toml /etc/redfire/gateway-node3.toml
# Edit: node_id = "node-03", logging path

# Start all nodes (they will form cluster automatically)
```

## 📊 Performance Tuning

### Call Capacity Guidelines
| Deployment Type | Max Calls | RTP Port Range | Memory (GB) |
|----------------|-----------|----------------|-------------|
| Basic          | 100       | 1000 ports     | 2          |
| Production     | 5000      | 10000 ports    | 8          |
| Mobile         | 10000     | 20000 ports    | 16         |
| Cluster Node   | 2500      | 5000 ports     | 6          |

### Network Requirements
- **Basic**: 100 Mbps network interface
- **Production**: Gigabit Ethernet
- **Mobile**: Gigabit Ethernet with QoS
- **Cluster**: Dedicated cluster network recommended

### Storage Requirements
- **Basic**: 10 GB for logs and state
- **Production**: 100 GB for logs, CDR, and monitoring data
- **Mobile**: 200 GB for enhanced logging and analytics
- **Cluster**: Additional storage for shared state backend

## 🔍 Troubleshooting

### Common Configuration Issues

#### Interface Not Found
```toml
[tdmoe]
interface = "eth0"  # Ensure interface exists: ip link show
```

#### Port Conflicts
```toml
[sip]
listen_port = 5060  # Ensure port is available: netstat -ln | grep 5060
```

#### Permission Issues
```bash
# Ensure proper permissions for log directory
sudo mkdir -p /var/log/redfire-gateway
sudo chown redfire:redfire /var/log/redfire-gateway
```

### Validation Commands
```bash
# Check configuration syntax
redfire-gateway --config gateway.toml --validate

# Test network interfaces
redfire-diag --interface eth0

# Check SIP connectivity (when integrated)
redfire-cli sip status
```

## 📞 Support

### Configuration Help
- **Documentation**: See main [README.md](../README.md)
- **Integration**: See [INTEGRATION.md](../INTEGRATION.md)
- **Issues**: [GitHub Issues](https://github.com/redfire/redfire-gateway/issues)

### Commercial Support
For production deployments and commercial support:
- **Email**: support@redfire.com
- **Commercial License**: licensing@redfire.com

---

**Note**: These examples are for the beta release. Some features require external library integration. See [INTEGRATION.md](../INTEGRATION.md) for complete setup instructions.