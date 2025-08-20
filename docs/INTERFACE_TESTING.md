# Interface Testing System

The Redfire Gateway includes a comprehensive interface testing system designed to validate TDMoE (TDM over Ethernet) connectivity, cross-port wiring, and end-to-end call functionality. This system is essential for ensuring reliable telecommunications operations.

## Overview

The interface testing system provides several key capabilities:

1. **TDMoE Loopback Testing** - Tests data transmission that loops back to itself
2. **Cross-Port Wiring Tests** - Validates connectivity between different TDM spans
3. **End-to-End Call Testing** - Simulates complete call scenarios
4. **Advanced Pattern Generation** - Multiple test patterns for various scenarios
5. **Automated Test Suites** - Orchestrated testing sequences with detailed analysis

## Architecture

The testing system consists of several key components:

- **InterfaceTestingService** - Core testing engine
- **TestAutomationService** - Orchestrates complex test scenarios
- **interface-test CLI** - Command-line interface for running tests
- **Test Pattern Generators** - Generates various signal patterns
- **Result Analysis Engine** - Analyzes test outcomes and provides recommendations

## Test Types

### 1. TDMoE Loopback Testing

Tests the internal loopback capability of TDM spans by sending data that should return to the same interface.

**Use Cases:**
- Validate TDM interface functionality
- Test signal processing pipelines
- Verify timing and synchronization
- Detect hardware issues

**Example:**
```bash
cargo run --bin interface-test loopback --span 1 --channels "1,2,3,4,5" --pattern prbs15 --duration 30
```

**Key Metrics:**
- Frame loss rate (should be < 0.1% for healthy systems)
- Bit error rate (should be < 1e-9)
- Timing jitter (should be < 50μs)
- Round-trip delay consistency

### 2. Cross-Port Wiring Tests

Validates connectivity between different TDM spans, essential for systems with multiple interfaces or redundant configurations.

**Use Cases:**
- Verify physical cabling between spans
- Test cross-connect functionality
- Validate redundancy configurations
- Detect crosstalk and interference

**Example:**
```bash
cargo run --bin interface-test cross-port --source-span 1 --dest-span 2 --mapping "1:1,2:2,3:3" --duration 60
```

**Key Metrics:**
- Cross-port connectivity (should achieve > 99% success rate)
- Signal degradation across connections
- Timing alignment between spans
- Interference levels

### 3. End-to-End Call Testing

Simulates complete call scenarios from one span to another, including signaling and media path validation.

**Use Cases:**
- Validate complete call flow
- Test voice quality metrics
- Verify signaling protocols (Q.931)
- Assess system capacity under load

**Example:**
```bash
cargo run --bin interface-test call-test --calling-span 1 --called-span 2 --frequency 1000 --duration 120
```

**Key Metrics:**
- Call setup success rate (should be > 99%)
- Voice quality (MOS score > 4.0)
- Call completion rate
- Post-dial delay

## Test Patterns

The system supports various test patterns optimized for different scenarios:

### PRBS (Pseudo-Random Binary Sequence)
- **PRBS-15**: 32,767-bit sequence, good for general testing
- **PRBS-23**: 8,388,607-bit sequence, comprehensive error detection
- **PRBS-31**: 2,147,483,647-bit sequence, maximum randomness

### Fixed Patterns
- **All Zeros**: Tests for stuck-at-zero conditions
- **All Ones**: Tests for stuck-at-one conditions  
- **Alternating**: 010101... pattern, tests timing recovery

### Protocol-Specific Patterns
- **Q.931 Setup**: Simulates call setup signaling
- **Q.931 Release**: Simulates call teardown
- **LAPD Frames**: Tests Layer 2 protocol handling

### Voice Simulation
- **Tone Generation**: Sine waves at specified frequencies
- **Speech Patterns**: Simulated voice characteristics

## Automated Test Suites

The system includes predefined test suites for common scenarios:

### Basic Connectivity
```bash
cargo run --bin interface-test suite "1,2,3" --loopback --cross-port --duration 30
```
- Loopback tests for each span
- Cross-port tests between all span pairs
- Basic connectivity validation

### System Validation
Comprehensive testing including:
- Multi-pattern loopback tests
- Cross-port validation with different patterns
- Protocol stack testing
- End-to-end call validation
- Optional stress testing

### Production Readiness
Extended testing for production environments:
- Long-duration stability tests
- High-volume call simulation
- Stringent quality thresholds
- Performance benchmarking

### Troubleshooting Scenarios
Targeted tests for specific issues:
- **High Latency**: Precision timing measurements
- **Packet Loss**: High-frequency loss detection
- **Bit Errors**: Multi-pattern error analysis
- **Sync Issues**: Timing synchronization validation
- **Crosstalk**: Adjacent span interference testing
- **Timing Drift**: Long-term stability analysis

## CLI Usage

### Basic Commands

**Run a loopback test:**
```bash
cargo run --bin interface-test loopback --span 1 --duration 30
```

**Test cross-port wiring:**
```bash
cargo run --bin interface-test cross-port --source-span 1 --dest-span 2 --pattern alternating
```

**Monitor active tests:**
```bash
cargo run --bin interface-test monitor --interval 2
```

**View test results:**
```bash
cargo run --bin interface-test results --detailed
```

**Stop a running test:**
```bash
cargo run --bin interface-test stop <test-id>
```

### Advanced Options

**Custom channel mapping:**
```bash
cargo run --bin interface-test cross-port --source-span 1 --dest-span 2 --mapping "1:5,2:6,3:7"
```

**Continuous testing:**
```bash
cargo run --bin interface-test loopback --span 1 --continuous
```

**Specific test patterns:**
```bash
cargo run --bin interface-test loopback --span 1 --pattern prbs31 --duration 60
```

## Interpreting Results

### Success Criteria

Tests are considered successful when they meet these thresholds:

**TDMoE Loopback:**
- Frame loss rate < 0.1%
- Bit error rate < 1e-6
- Average delay < 50ms
- Jitter < 10ms

**Cross-Port Wiring:**
- Success rate > 99%
- Bit error rate < 1e-6
- Signal degradation < 3dB

**End-to-End Calls:**
- Call setup success > 95%
- MOS score > 4.0
- Post-dial delay < 3s

### Common Issues and Recommendations

**High Frame Loss:**
- Check physical connections
- Verify cable integrity
- Inspect network congestion

**Excessive Bit Errors:**
- Check signal levels
- Reduce electromagnetic interference
- Verify timing synchronization

**High Latency:**
- Optimize processing pipeline
- Reduce buffering delays
- Check network path

**Timing Issues:**
- Verify clock sources
- Check synchronization configuration
- Inspect timing distribution

## Integration with Diagnostics

The interface testing system integrates with the gateway's diagnostic tools:

**Real-time monitoring:**
```bash
cargo run --bin redfire-diag system
```

**Protocol analysis:**
```bash
cargo run --bin redfire-diag tdm d-channel --span 1
```

**Channel status:**
```bash
cargo run --bin redfire-diag channels status
```

## API Usage

For programmatic access, use the Rust API:

```rust
use redfire_gateway::services::{InterfaceTestingService, TestPattern};
use std::time::Duration;

// Create testing service
let service = InterfaceTestingService::new();

// Start loopback test
let test_id = service.start_tdmoe_loopback_test(
    1,                              // span
    Some(vec![1, 2, 3]),           // channels
    TestPattern::Prbs15,           // pattern
    Duration::from_secs(30),       // duration
).await?;

// Monitor progress
while let Some(stats) = service.get_test_status(test_id).await {
    println!("Progress: {} frames sent", stats.frames_sent);
    tokio::time::sleep(Duration::from_secs(1)).await;
}

// Get results
let result = service.get_test_result(test_id).await.unwrap();
println!("Test success: {}", result.success);
```

## Best Practices

### Pre-Deployment Testing
1. Run basic connectivity tests on all spans
2. Validate cross-port wiring configurations
3. Perform end-to-end call tests
4. Execute stress tests under load
5. Document all test results

### Maintenance Testing
1. Schedule regular automated test suites
2. Monitor test trends over time
3. Set up alerting for test failures
4. Perform targeted troubleshooting tests
5. Validate after any configuration changes

### Production Monitoring
1. Implement continuous background testing
2. Set appropriate test intervals
3. Monitor key quality metrics
4. Correlate test results with service quality
5. Maintain test result history

## Troubleshooting

### Common Test Failures

**"Test failed to start":**
- Verify span configuration
- Check system resources
- Ensure no conflicting tests

**"High frame loss detected":**
- Inspect physical connections
- Check for network congestion
- Verify timing synchronization

**"Bit errors exceeding threshold":**
- Check signal quality
- Reduce interference sources
- Verify cable specifications

### Debug Mode

Enable detailed debugging:
```bash
RUST_LOG=debug cargo run --bin interface-test loopback --span 1
```

### Log Analysis

Test logs provide detailed information:
- Frame transmission timestamps
- Error detection events
- Quality measurements
- Performance statistics

## Performance Considerations

### System Load
- Tests consume CPU and memory resources
- Limit concurrent tests based on system capacity
- Monitor system performance during testing

### Network Impact
- TDMoE tests generate network traffic
- Consider bandwidth limitations
- Coordinate with network operations

### Test Duration
- Longer tests provide more accurate results
- Balance accuracy with operational impact
- Use appropriate test intervals

## Security Considerations

### Access Control
- Restrict test execution to authorized users
- Log all test activities
- Implement proper authentication

### Network Security
- Test traffic should not interfere with production
- Use appropriate network isolation
- Monitor for security implications

### Data Protection
- Test results may contain sensitive information
- Implement appropriate data retention policies
- Ensure secure result storage

## Future Enhancements

The interface testing system is designed for extensibility:

- Additional test patterns
- Enhanced automation capabilities
- Integration with external test equipment
- Machine learning for predictive analysis
- Cloud-based result aggregation

## Support and Maintenance

For technical support:
- Review test logs and results
- Check system configuration
- Verify hardware connectivity
- Contact support with detailed test output

Regular maintenance:
- Update test patterns as needed
- Calibrate timing references
- Validate test result accuracy
- Review and update success criteria