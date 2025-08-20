#!/bin/bash

# Redfire Gateway Debian Package Build Script

set -e

echo "🚀 Building Redfire Gateway Debian Package"
echo "=========================================="

# Check if running on Debian/Ubuntu
if ! command -v dpkg-buildpackage &> /dev/null; then
    echo "❌ Error: dpkg-buildpackage not found. This script requires Debian/Ubuntu."
    echo "   Install with: sudo apt install dpkg-dev"
    exit 1
fi

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo "❌ Error: Rust/Cargo not found. Please install Rust first."
    echo "   Install with: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check for required build dependencies
echo "📦 Checking build dependencies..."
MISSING_DEPS=""

for dep in build-essential debhelper dh-systemd; do
    if ! dpkg -l "$dep" &> /dev/null; then
        MISSING_DEPS="$MISSING_DEPS $dep"
    fi
done

if [ -n "$MISSING_DEPS" ]; then
    echo "❌ Missing build dependencies:$MISSING_DEPS"
    echo "   Install with: sudo apt install$MISSING_DEPS"
    exit 1
fi

# Clean any previous builds
echo "🧹 Cleaning previous builds..."
cargo clean
rm -rf target/debian
rm -f ../redfire-gateway*.deb ../redfire-gateway*.changes ../redfire-gateway*.tar.* ../redfire-gateway*.dsc

# Build the package
echo "🔨 Building Debian package..."
echo "   This may take several minutes for the initial build..."

# Set build environment
export DEB_BUILD_OPTIONS="parallel=$(nproc)"
export CARGO_INCREMENTAL=0
export RUST_BACKTRACE=1

# Build the package
dpkg-buildpackage -us -uc -b

if [ $? -eq 0 ]; then
    echo ""
    echo "✅ Build completed successfully!"
    echo ""
    echo "📦 Package files created:"
    ls -la ../redfire-gateway*.deb 2>/dev/null || echo "   (No .deb files found - check build output)"
    echo ""
    echo "📥 To install the package:"
    echo "   sudo dpkg -i ../redfire-gateway*.deb"
    echo "   sudo apt-get install -f  # Fix any dependency issues"
    echo ""
    echo "🔧 After installation:"
    echo "   sudo systemctl status redfire-gateway"
    echo "   sudo systemctl start redfire-gateway"
    echo "   redfire-cli status"
    echo ""
    echo "📚 Documentation:"
    echo "   /usr/share/doc/redfire-gateway/"
    echo "   Configuration: /etc/redfire-gateway/gateway.toml"
    echo "   Logs: /var/log/redfire-gateway/"
else
    echo ""
    echo "❌ Build failed. Check the output above for errors."
    echo ""
    echo "🔍 Common issues:"
    echo "   - Missing Rust dependencies: cargo update"
    echo "   - Missing system dependencies: sudo apt install build-essential"
    echo "   - Insufficient disk space: df -h"
    echo ""
    exit 1
fi