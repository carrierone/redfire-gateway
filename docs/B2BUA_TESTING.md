# B2BUA Testing Framework

This document describes the comprehensive testing framework for the Redfire Gateway B2BUA implementation using standard VoIP testing tools.

## Overview

The B2BUA testing framework uses industry-standard tools to validate the media translation, transcoding, and call management features:

- **SIPp**: SIP protocol testing and call generation
- **FFmpeg**: Media file generation and analysis
- **Test Runner**: Rust-based orchestration tool
- **Automated Scripts**: Bash scripts for comprehensive test execution

## Prerequisites

### Required Tools

1. **SIPp** - SIP testing tool
   ```bash
   # Ubuntu/Debian
   sudo apt-get install sipp
   
   # CentOS/RHEL
   sudo yum install sipp
   
   # macOS
   brew install sipp
   ```

2. **FFmpeg** - Media processing (optional but recommended)
   ```bash
   # Ubuntu/Debian
   sudo apt-get install ffmpeg
   
   # CentOS/RHEL
   sudo yum install ffmpeg
   
   # macOS
   brew install ffmpeg
   ```

3. **Additional Tools** (optional)
   ```bash
   # For advanced reporting
   sudo apt-get install jq pandoc bc netcat-openbsd
   ```

### Network Configuration

Ensure the following ports are available:
- SIP: 5060-5090 (UDP)
- RTP: 10000-20000 (UDP)
- Gateway: Configured port (default 5060)

## Quick Start

### 1. Build the Test Tools

```bash
cd /path/to/redfire-gateway
cargo build --release --bin test-runner
```

### 2. Start the Gateway

Ensure the Redfire Gateway B2BUA is running:

```bash
# Start the gateway (example)
cargo run --bin redfire-gateway -- --config config/gateway.toml
```

### 3. Run Basic Tests

```bash
# Run all tests
./scripts/run-b2bua-tests.sh

# Run specific test types
./scripts/run-b2bua-tests.sh --only basic
./scripts/run-b2bua-tests.sh --only transcoding
```

## Test Types

### 1. Basic Call Tests

Tests fundamental SIP call flow and media relay:

```bash
./target/release/test-runner basic-call \\
    --calls 5 \\
    --duration 30 \\
    --codec g711u \\
    --gateway 127.0.0.1:5060
```

**Validates:**
- INVITE/ACK/BYE flow
- SDP negotiation
- RTP media flow
- Call state management

### 2. Transcoding Tests

Tests codec conversion capabilities:

```bash
./target/release/test-runner transcoding \\
    --from-codec g711u \\
    --to-codec g711a \\
    --duration 60 \\
    --gateway 127.0.0.1:5060
```

**Supported Codecs:**
- G.711u (μ-law)
- G.711a (A-law)
- G.722 (wideband)
- G.729 (compressed)
- Opus (modern)
- Speex (legacy)

**Validates:**
- Codec negotiation
- Real-time transcoding
- Audio quality preservation
- GPU acceleration (if available)

### 3. DTMF Testing

Tests DTMF detection and relay:

```bash
./target/release/test-runner dtmf \\
    --sequence "123456789*0#ABCD" \\
    --method rfc2833 \\
    --gateway 127.0.0.1:5060
```

**DTMF Methods:**
- RFC 2833 (RTP events)
- SIP INFO
- In-band audio

**Validates:**
- DTMF detection accuracy
- End-to-end relay
- Timing preservation

### 4. Stress Testing

Tests system performance under load:

```bash
./target/release/test-runner stress \\
    --concurrent 50 \\
    --total 500 \\
    --rate 10 \\
    --duration 120 \\
    --gateway 127.0.0.1:5060
```

**Validates:**
- Concurrent call handling
- Memory management
- CPU utilization
- Call success rate
- System stability

### 5. Quality Testing

Tests media quality under various network conditions:

```bash
./target/release/test-runner quality \\
    --packet-loss 1.0 \\
    --jitter 50 \\
    --delay 100 \\
    --gateway 127.0.0.1:5060
```

**Network Conditions:**
- Packet loss simulation
- Jitter introduction
- Latency simulation
- Bandwidth constraints

### 6. Media Generation

Creates test media files for validation:

```bash
./target/release/test-runner generate-media tone1000 \\
    --duration 30 \\
    --format wav \\
    --output-dir ./test-media
```

**Media Types:**
- Sine wave tones (440Hz, 1000Hz)
- DTMF sequences
- White noise
- Voice samples
- Music samples

## SIPp Scenarios

The framework includes pre-built SIPp scenarios for comprehensive testing:

### UAC (User Agent Client) Scenario
- Initiates outbound calls
- Sends RTP media
- Handles call termination

### UAS (User Agent Server) Scenario  
- Receives inbound calls
- Echoes RTP media
- Responds to termination

### DTMF Test Scenario
- Sends DTMF via RTP events
- Sends DTMF via SIP INFO
- Validates DTMF relay

### Codec Negotiation Scenario
- Offers multiple codecs
- Validates codec selection
- Tests transcoding paths

## Test Configuration

### Environment Variables

```bash
export GATEWAY_HOST="127.0.0.1"
export GATEWAY_PORT="5060"
export BIND_ADDRESS="127.0.0.1"
export OUTPUT_DIR="./test-results"
export SIPP_PATH="/usr/bin/sipp"
export FFMPEG_PATH="/usr/bin/ffmpeg"
```

### Custom Test Suites

Create JSON configuration files for custom test suites:

```json
{
  "name": "Custom B2BUA Test Suite",
  "description": "Custom validation tests",
  "tests": [
    {
      "name": "high_concurrency",
      "test_type": "stress",
      "parameters": {
        "concurrent": 100,
        "total": 1000,
        "rate": 20,
        "duration": 300
      },
      "expected_results": {
        "success_rate_percent": 99.0,
        "max_response_time_ms": 1000
      }
    }
  ]
}
```

## Results and Reporting

### Test Results Structure

```
test-results/
├── test_results.json          # Detailed JSON results
├── summary_report.md          # Markdown summary
├── test_report.html          # HTML report (if pandoc available)
├── scenarios/                # Generated SIPp scenarios
│   ├── uac_basic.xml
│   ├── uas_basic.xml
│   ├── dtmf_test.xml
│   └── codec_test.xml
├── media/                    # Generated media files
│   ├── tone1000_30.wav
│   ├── dtmf_30.wav
│   └── noise_30.raw
├── logs/                     # Test execution logs
│   ├── sip_messages.log
│   ├── stress_test.log
│   └── test_runner.log
└── captures/                 # Network captures (if available)
    ├── test_capture.pcap
    └── rtp_analysis.txt
```

### Metrics Collected

**Call Metrics:**
- Total calls attempted
- Successful call completions
- Failed calls with reasons
- Average call duration
- Call setup time
- Success rate percentage

**Media Metrics:**
- RTP packets sent/received
- Packet loss percentage
- Jitter measurements
- MOS (Mean Opinion Score)
- Codec negotiation results
- Transcoding performance

**System Metrics:**
- CPU utilization
- Memory usage
- Network bandwidth
- Concurrent call capacity
- Response times

## Advanced Testing

### Network Emulation

Use Linux traffic control for network condition simulation:

```bash
# Add packet loss
sudo tc qdisc add dev eth0 root netem loss 1%

# Add jitter
sudo tc qdisc add dev eth0 root netem delay 100ms 20ms

# Add bandwidth limitation
sudo tc qdisc add dev eth0 root handle 1: tbf rate 1mbit burst 32kbit latency 400ms
```

### Continuous Integration

Example GitHub Actions workflow:

```yaml
name: B2BUA Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y sipp ffmpeg jq
      - name: Build gateway
        run: cargo build --release
      - name: Run tests
        run: |
          cargo run --bin redfire-gateway &
          sleep 10
          ./scripts/run-b2bua-tests.sh --skip-stress
      - name: Upload results
        uses: actions/upload-artifact@v2
        with:
          name: test-results
          path: test-results/
```

### Docker Testing

Run tests in isolated containers:

```dockerfile
FROM ubuntu:20.04

RUN apt-get update && apt-get install -y \\
    sipp ffmpeg jq netcat-openbsd \\
    && rm -rf /var/lib/apt/lists/*

COPY target/release/test-runner /usr/local/bin/
COPY scripts/run-b2bua-tests.sh /usr/local/bin/

CMD ["/usr/local/bin/run-b2bua-tests.sh"]
```

## Troubleshooting

### Common Issues

1. **SIPp Connection Refused**
   ```
   Error: Connection refused to gateway
   Solution: Verify gateway is running and accessible
   ```

2. **RTP Media Not Flowing**
   ```
   Error: No RTP packets received
   Solution: Check firewall rules and RTP port ranges
   ```

3. **Transcoding Test Failures**
   ```
   Error: Codec negotiation failed
   Solution: Verify codec support in gateway configuration
   ```

### Debug Mode

Enable verbose logging:

```bash
./target/release/test-runner --verbose transcoding \\
    --from-codec g711u --to-codec g722
```

### Network Analysis

Capture and analyze network traffic:

```bash
# Capture SIP/RTP traffic
sudo tcpdump -i any -w test_capture.pcap 'port 5060 or portrange 10000-20000'

# Analyze with Wireshark
wireshark test_capture.pcap
```

## Performance Benchmarks

### Expected Performance Metrics

**Basic Calls:**
- Setup time: < 100ms
- Success rate: > 99%
- Concurrent calls: 100+ (depending on hardware)

**Transcoding:**
- G.711 ↔ G.711: < 1ms latency
- G.711 ↔ G.722: < 5ms latency
- GPU acceleration: 10x improvement

**Stress Testing:**
- 1000 concurrent calls
- 10,000 calls/hour throughput
- < 1% packet loss under normal conditions

## Contributing

### Adding New Tests

1. Create new test scenarios in `src/bin/test-runner.rs`
2. Add corresponding SIPp XML scenarios
3. Update the bash script for integration
4. Document the new test type

### Test Data

Provide sample media files and SIP traces for validation testing.

## References

- [SIPp Documentation](http://sipp.sourceforge.net/)
- [RFC 3261 - SIP Protocol](https://tools.ietf.org/html/rfc3261)
- [RFC 3550 - RTP Protocol](https://tools.ietf.org/html/rfc3550)
- [RFC 2833 - DTMF Events](https://tools.ietf.org/html/rfc2833)
- [FFmpeg Documentation](https://ffmpeg.org/documentation.html)