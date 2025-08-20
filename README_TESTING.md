# Redfire Gateway B2BUA Testing Framework

A comprehensive testing framework for validating B2BUA media translation, transcoding, and telephony features using industry-standard tools.

## 🚀 Quick Start

### Prerequisites
```bash
# Install required tools
sudo apt-get install sipp ffmpeg jq netcat-openbsd

# Build the test framework
cargo build --release --bin test-runner
```

### Run Basic Tests
```bash
# Start the gateway
cargo run --bin redfire-gateway &

# Run comprehensive test suite
./scripts/run-b2bua-tests.sh
```

## 📋 What Gets Tested

### ✅ Core Functionality
- **Basic Call Flow**: SIP INVITE/ACK/BYE sequences
- **Media Relay**: RTP packet forwarding and processing
- **Session Management**: Call state tracking and correlation
- **Protocol Compliance**: SIP and RTP standard conformance

### 🔄 Transcoding Features
- **Codec Conversion**: G.711u ↔ G.711a ↔ G.722 ↔ Opus
- **GPU Acceleration**: CUDA/ROCm performance validation
- **Quality Preservation**: MOS score and audio fidelity
- **Real-time Processing**: Latency and throughput metrics

### 📞 DTMF Testing
- **RFC 2833**: RTP event-based DTMF
- **SIP INFO**: SIP message-based DTMF
- **In-band Detection**: Audio frequency analysis
- **End-to-end Relay**: Accuracy and timing validation

### 🏋️ Performance & Stress
- **Concurrent Calls**: Up to 1000+ simultaneous sessions
- **Call Rate**: High CPS (Calls Per Second) testing
- **Memory Management**: Leak detection and optimization
- **System Stability**: Long-duration stress testing

### 🌐 Network Resilience
- **Packet Loss**: Behavior under 0-5% loss conditions
- **Jitter Tolerance**: Variable delay compensation
- **Latency Impact**: High-latency network simulation
- **Quality Metrics**: Real-time MOS calculation

## 🛠️ Testing Tools

### SIPp Integration
- **Industry Standard**: Widely used VoIP testing tool
- **Realistic Traffic**: Authentic SIP protocol behavior
- **Scenario Flexibility**: Custom test scenarios
- **Statistics Collection**: Detailed performance metrics

### FFmpeg Media Generation
- **Test Signals**: Tones, noise, DTMF sequences
- **Format Support**: WAV, raw audio, multiple codecs
- **Quality Analysis**: Audio comparison and validation
- **Automated Processing**: Batch media generation

### Custom Test Runner
- **Orchestration**: Coordinates multiple tools
- **Result Analysis**: Parses and correlates metrics
- **Report Generation**: HTML, JSON, and markdown
- **CI/CD Integration**: Automated test execution

## 📊 Test Results

### Metrics Collected
```json
{
  "call_metrics": {
    "total_calls": 500,
    "successful_calls": 498,
    "success_rate_percent": 99.6,
    "average_setup_time_ms": 85,
    "average_call_duration": 45.2
  },
  "media_metrics": {
    "packets_relayed": 1250000,
    "packet_loss_rate": 0.1,
    "average_jitter_ms": 12.5,
    "mos_score": 4.2
  },
  "transcoding_metrics": {
    "sessions_transcoded": 125,
    "gpu_utilization_percent": 35.8,
    "transcoding_latency_ms": 3.2,
    "quality_degradation_percent": 2.1
  }
}
```

### Visual Reports
- **Call Success Rates**: Timeline graphs
- **Quality Metrics**: MOS score trends  
- **Performance Charts**: CPU, memory, throughput
- **Error Analysis**: Failure categorization

## 🎯 Test Scenarios

### Basic Validation
```bash
# Simple call test
./target/release/test-runner basic-call \
    --calls 10 --duration 30 --codec g711u

# Transcoding test  
./target/release/test-runner transcoding \
    --from-codec g711u --to-codec g722 --duration 60
```

### Advanced Testing
```bash
# Stress test with 100 concurrent calls
./target/release/test-runner stress \
    --concurrent 100 --total 1000 --rate 10

# Network quality simulation
./target/release/test-runner quality \
    --packet-loss 2.0 --jitter 50 --delay 100
```

### Custom Test Suites
```bash
# Run predefined comprehensive suite
./target/release/test-runner suite \
    --config test-configs/comprehensive-suite.json \
    --include-stress
```

## 🔧 Configuration

### Environment Variables
```bash
export GATEWAY_HOST="127.0.0.1"
export GATEWAY_PORT="5060"
export SIPP_PATH="/usr/bin/sipp"
export FFMPEG_PATH="/usr/bin/ffmpeg"
export OUTPUT_DIR="./test-results"
```

### Gateway Configuration
Ensure your gateway is configured for testing:
```toml
[b2bua]
enabled = true
max_concurrent_calls = 200
transcoding_enabled = true
gpu_acceleration = true

[sip]
bind_address = "0.0.0.0"
port = 5060

[rtp]
port_range = { min = 10000, max = 20000 }
```

## 📈 Performance Benchmarks

### Expected Results
- **Call Setup**: < 100ms average
- **Success Rate**: > 99% under normal conditions
- **Concurrent Capacity**: 100+ calls (hardware dependent)
- **Transcoding Latency**: < 10ms for G.711 conversion
- **GPU Acceleration**: 5-10x performance improvement

### Hardware Requirements
- **Minimum**: 4 CPU cores, 8GB RAM
- **Recommended**: 8+ CPU cores, 16GB+ RAM
- **GPU Transcoding**: CUDA or ROCm compatible card
- **Network**: Gigabit Ethernet recommended

## 🐛 Troubleshooting

### Common Issues

**SIPp Connection Failed**
```bash
# Check if gateway is running
nc -z 127.0.0.1 5060

# Verify firewall rules
sudo ufw status
```

**RTP Media Issues**
```bash
# Check RTP port availability
netstat -un | grep 10000-20000

# Monitor RTP traffic
sudo tcpdump -i any 'portrange 10000-20000'
```

**Transcoding Failures**
```bash
# Verify GPU drivers (for GPU acceleration)
nvidia-smi  # For CUDA
rocm-smi   # For ROCm

# Check codec support
./target/release/test-runner generate-media tone1000
```

### Debug Mode
```bash
# Enable verbose logging
RUST_LOG=debug ./target/release/test-runner basic-call --verbose

# SIPp debug output
sipp -trace_msg -message_file debug.log
```

## 🔄 Continuous Integration

### GitHub Actions Example
```yaml
name: B2BUA Tests
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Install tools
        run: sudo apt-get install -y sipp ffmpeg
      - name: Run tests
        run: ./scripts/run-b2bua-tests.sh --skip-stress
```

### Docker Testing
```bash
# Build test container
docker build -t b2bua-tests .

# Run isolated tests
docker run --rm -v $(pwd)/test-results:/results b2bua-tests
```

## 📚 Documentation

- **[Complete Testing Guide](docs/B2BUA_TESTING.md)**: Detailed documentation
- **[SIPp Scenarios](test-results/scenarios/)**: Generated test scenarios
- **[Configuration Examples](test-configs/)**: Sample test configurations
- **[API Reference](docs/API.md)**: Test runner API documentation

## 🤝 Contributing

### Adding Tests
1. Implement test logic in `src/bin/test-runner.rs`
2. Create SIPp scenarios if needed
3. Update test scripts and documentation
4. Add validation for expected results

### Reporting Issues
- Include test logs and configuration
- Specify gateway version and environment
- Provide network topology details

## 📄 License

This testing framework is part of the Redfire Gateway project and follows the same license terms.

---

**Ready to test your B2BUA implementation?** Start with the quick start guide above! 🎉