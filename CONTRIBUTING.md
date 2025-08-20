# Contributing to Redfire Gateway

Thank you for your interest in contributing to Redfire Gateway! This document provides guidelines and information for contributors.

## 📄 License and Legal

**Important**: Redfire Gateway uses a dual-license model:

### GPL v3 Contributions (Open Source)
- By contributing to the open source version, you agree to license your contributions under GPL v3
- Your contributions may be included in both the open source and commercial versions
- You retain copyright to your contributions

### Commercial License Considerations
- Contributions may be included in commercially licensed versions
- Commercial license holders receive additional support and services
- Contact [licensing@redfire.com](mailto:licensing@redfire.com) for commercial development opportunities

### Contributor License Agreement
Before your first contribution, you'll need to:
1. Agree that your contributions will be licensed under GPL v3
2. Confirm you have the right to make the contribution
3. Understand that contributions may be included in commercial versions

## 🚀 Getting Started

### Development Environment Setup

1. **Install Rust** (1.70.0 or later):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source ~/.cargo/env
   ```

2. **Install system dependencies**:
   ```bash
   # Ubuntu/Debian
   sudo apt-get update
   sudo apt-get install -y \
     libpcap-dev \
     libsnmp-dev \
     pkg-config \
     build-essential

   # Other distributions: install equivalent packages
   ```

3. **Clone and build**:
   ```bash
   git clone https://github.com/redfire/redfire-gateway.git
   cd redfire-gateway
   cargo build
   cargo test --lib  # Core tests should pass in beta
   ```

### Project Structure

```
redfire-gateway/
├── src/
│   ├── lib.rs              # Library entry point
│   ├── main.rs             # Main gateway binary
│   ├── config.rs           # Configuration management
│   ├── protocols/          # Protocol implementations
│   │   ├── sip.rs         # SIP protocol (stub)
│   │   ├── rtp.rs         # RTP/RTCP handling
│   │   └── tdmoe.rs       # TDM over Ethernet
│   ├── services/           # Core services
│   │   ├── transcoding.rs # Transcoding (stub)
│   │   ├── b2bua.rs       # Back-to-back user agent
│   │   ├── clustering.rs  # High availability
│   │   └── monitoring.rs  # Performance monitoring
│   └── bin/               # CLI utilities
├── examples/              # Configuration examples
├── docs/                  # Documentation
├── tests/                 # Integration tests
├── INTEGRATION.md         # External library integration guide
└── README.md             # Project overview
```

## 🎯 Types of Contributions

### 1. Bug Fixes
- Fix compilation errors or runtime bugs
- Improve error handling and logging
- Performance optimizations
- Memory safety improvements

### 2. Feature Development
- New telecommunications protocols
- Enhanced monitoring and management
- Integration improvements
- Testing tools and utilities

### 3. Integration Work
- External SIP library integration examples
- Transcoding library integration examples
- Hardware interface improvements
- Protocol compliance enhancements

### 4. Documentation
- Code documentation and comments
- Integration guides and examples
- Configuration documentation
- Tutorial content

### 5. Testing
- Unit test improvements
- Integration test development
- Performance benchmarking
- Hardware compatibility testing

## 📝 Contribution Workflow

### 1. Issue First
- **Check existing issues** before creating new ones
- **Create an issue** for bugs, features, or questions
- **Discuss approach** for significant changes
- **Get feedback** before starting major work

### 2. Development Process

1. **Fork and branch**:
   ```bash
   git fork https://github.com/redfire/redfire-gateway.git
   git checkout -b feature/your-feature-name
   ```

2. **Make changes**:
   - Follow the code style guidelines (see below)
   - Add tests for new functionality
   - Update documentation as needed
   - Ensure all tests pass

3. **Test thoroughly**:
   ```bash
   # Core library tests (should pass)
   cargo test --lib
   
   # Integration tests (may fail in beta)
   cargo test
   
   # Check formatting and linting
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   
   # Build documentation
   cargo doc --lib --no-deps
   ```

4. **Commit and push**:
   ```bash
   git add .
   git commit -m "feat: add new feature description"
   git push origin feature/your-feature-name
   ```

5. **Create Pull Request**:
   - Use the PR template
   - Provide clear description of changes
   - Link to related issues
   - Include testing information

### 3. Code Review Process
- All contributions require code review
- Maintainers will provide feedback
- Address feedback and update PR
- Once approved, code will be merged

## 🎨 Code Style Guidelines

### Rust Code Style
- **Use `cargo fmt`** for consistent formatting
- **Follow Rust naming conventions**:
  - `snake_case` for functions and variables
  - `PascalCase` for types and traits
  - `SCREAMING_SNAKE_CASE` for constants
- **Write idiomatic Rust**:
  - Use `?` operator for error handling
  - Prefer `Result<T, E>` over panics
  - Use `async/await` for async code
  - Leverage the type system for safety

### Documentation
- **Document public APIs** with `///` comments
- **Include examples** in documentation
- **Update README.md** for user-facing changes
- **Update INTEGRATION.md** for integration changes

### Error Handling
- **Use appropriate error types** from `src/error.rs`
- **Provide meaningful error messages**
- **Log errors with context**
- **Avoid unwrap() in production code**

### Testing
- **Write unit tests** for all new functionality
- **Use `#[cfg(test)]` modules** for test code
- **Test both success and error paths**
- **Include integration tests** for major features

### Performance
- **Avoid unnecessary allocations**
- **Use zero-copy techniques** where possible
- **Profile performance-critical code**
- **Document performance characteristics**

## 🧪 Testing Guidelines

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_functionality() {
        // Test implementation
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn test_async_functionality() {
        // Async test implementation
        let result = async_function().await;
        assert!(result.is_ok());
    }
}
```

### Integration Tests
- Place in `tests/` directory
- Test complete workflows
- May fail in beta due to stub implementations
- Include external library integration tests

### Performance Tests
- Use `cargo bench` for benchmarks
- Profile memory usage
- Test under load
- Document performance characteristics

## 🔒 Security Guidelines

### Security Best Practices
- **Never commit secrets** or credentials
- **Validate all inputs** from external sources
- **Use secure defaults** in configuration
- **Follow principle of least privilege**
- **Report security issues privately** to security@redfire.com

### Telecommunications Security
- **Implement proper authentication** for SIP
- **Support TLS/SRTP** encryption
- **Validate protocol compliance**
- **Implement rate limiting** and DDoS protection

## 📋 Specific Contribution Areas

### 1. Beta Integration Work
The current priority is integrating external libraries:

**SIP Integration**:
- Implement `SipHandler` trait for external SIP libraries
- Create integration examples
- Update CLI tools for new architecture
- Test with real SIP traffic

**Transcoding Integration**:
- Implement `TranscodingService` for external libraries
- Support GPU acceleration (CUDA/ROCm)
- Performance optimization
- Codec compatibility testing

### 2. Core Infrastructure
- Performance monitoring improvements
- SNMP MIB enhancements
- Clustering and HA features
- Configuration management

### 3. Telecommunications Protocols
- SS7/SigTran improvements
- Mobile network integration
- Protocol compliance testing
- Hardware interface support

### 4. Tools and Utilities
- CLI tool improvements
- Diagnostic utilities
- Testing frameworks
- Management interfaces

## 🐛 Bug Reports

When reporting bugs, include:

### System Information
- OS and version
- Rust version
- Hardware specifications
- Network configuration

### Reproduction Steps
1. Exact configuration used
2. Commands executed
3. Expected vs actual behavior
4. Error messages and logs

### Integration Details
- External libraries used
- Integration method
- Custom modifications

## 💡 Feature Requests

For feature requests, provide:

### Use Case Description
- Problem being solved
- Target users/scenarios
- Business justification

### Technical Requirements
- Performance requirements
- Compatibility needs
- Regulatory considerations

### Implementation Ideas
- Proposed approach
- Alternative solutions
- Technical challenges

## 🤝 Community Guidelines

### Code of Conduct
- **Be respectful** and inclusive
- **Collaborate constructively**
- **Focus on technical merit**
- **Help newcomers** learn and contribute

### Communication Channels
- **GitHub Issues**: Bug reports and feature requests
- **GitHub Discussions**: General questions and ideas
- **Pull Requests**: Code contributions and reviews

### Commercial vs. Open Source
- **Open source discussions** happen in public
- **Commercial support** available separately
- **Professional consulting** for complex integrations
- **Contact licensing@redfire.com** for commercial needs

## 📚 Resources

### Documentation
- [README.md](README.md) - Project overview
- [INTEGRATION.md](INTEGRATION.md) - Integration guide
- [CHANGELOG.md](CHANGELOG.md) - Version history

### External Resources
- [Rust Programming Language](https://www.rust-lang.org/)
- [Telecommunications Standards](https://www.itu.int/)
- [SIP Protocol (RFC 3261)](https://tools.ietf.org/html/rfc3261)
- [RTP/RTCP (RFC 3550)](https://tools.ietf.org/html/rfc3550)

### Development Tools
- **IDE**: VS Code with rust-analyzer
- **Debugging**: GDB, Valgrind
- **Profiling**: perf, flamegraph
- **Documentation**: rustdoc

## 🎉 Recognition

Contributors will be:
- **Listed in CHANGELOG.md** for their contributions
- **Credited in release notes**
- **Invited to beta testing** programs
- **Considered for commercial opportunities**

### Top Contributors
- May receive **commercial license benefits**
- **Priority support** for their projects
- **Collaboration opportunities** with the core team

---

## 📞 Getting Help

### For Contributors
- **GitHub Discussions**: General questions
- **GitHub Issues**: Specific problems
- **Documentation**: Check existing guides first

### For Commercial Users
- **Email**: support@redfire.com
- **Priority support** with commercial license
- **Professional consulting** available

### For Security Issues
- **Email**: security@redfire.com
- **Private disclosure** encouraged
- **Coordinated vulnerability disclosure**

---

Thank you for contributing to Redfire Gateway! Your contributions help build better telecommunications infrastructure for everyone.