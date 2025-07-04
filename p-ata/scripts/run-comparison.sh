#!/bin/bash

set -e

echo "🔨 P-ATA vs Original ATA Comparison Script"
echo "=========================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print colored output
print_status() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ] || ! grep -q "pinocchio-ata-program" Cargo.toml; then
    print_error "This script must be run from the p-ata directory"
    exit 1
fi

print_status "Building both P-ATA and Original ATA programs..."

# Build with build-programs feature to compile both implementations
if cargo bench --features build-programs --bench ata_instruction_benches; then
    print_success "Comparison benchmarks completed successfully!"
    
else
    print_error "Benchmark run failed!"
    echo ""
    echo "🔧 Common issues and solutions:"
    echo "   • Missing submodules: git submodule update --init --recursive"
    echo "   • Missing solana tools: Install solana CLI and ensure 'cargo build-sbf' works"
    echo ""
    exit 1
fi

# Offer to run failure scenarios as well
read -p "🧪 Would you like to run failure scenario tests as well? [y/N]: " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    print_status "Running failure scenario benchmarks..."
    if cargo bench --features build-programs --bench failure_scenarios; then
        print_success "Failure scenario tests completed!"
    else
        print_warning "Failure scenario tests had issues"
    fi
fi

print_success "All benchmarking completed!"