# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2025-01-XX

### Added

**Core Architecture**
- Complete TDM over Ethernet (TDMoE) gateway infrastructure
- High-performance async Rust implementation
- Event-driven system with comprehensive monitoring

**Telecommunications Protocols**
- E1/T1 interface support with configurable framing and line coding
- ISDN PRI support (ETSI, NI2, ANSI variants)
- Channel Associated Signaling (CAS)
- SS7/SigTran for carrier-grade deployments
- FreeTDM integration for hardware abstraction

**VoIP Infrastructure**
- SIP protocol support via external redfire-sip-stack integration
- RTP/RTCP media handling with jitter buffer management
- Professional codec transcoding via external redfire-codec-engine
- DTMF handling (RFC2833, SIP INFO, in-band)
- Media relay and B2BUA functionality

**Enterprise Features**
- Clustering and high availability with anycast support
- Load balancing across multiple gateway instances
- Call detail records (CDR) with comprehensive billing information
- Performance monitoring with configurable thresholds
- SNMP management interface
- Configuration management via TOML files and environment variables

**Management Tools**
- CLI management interface (`redfire-cli`)
- Diagnostic tools (`redfire-diag`)
- Interface testing utility (`interface-test`)
- B2BUA management CLI (`b2bua-cli`)
- Test automation and benchmarking tools

**Documentation**
- Comprehensive README with installation and configuration guide
- API documentation
- Configuration examples

### Technical Specifications
- **Minimum Rust Version**: 1.70+
- **Target Architecture**: x86_64 Linux
- **Memory Requirements**: 4GB minimum, 8GB recommended
- **Network Requirements**: Gigabit Ethernet recommended for TDMoE

### License
- **GPL v3**: Open source license
- **Professional Support**: Available through commercial arrangements