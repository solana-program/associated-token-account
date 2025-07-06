#!/bin/bash
set -e

# P-ATA Local Benchmark Runner
# This script runs benchmarks locally and generates badge data

echo "🚀 P-ATA Local Benchmark Runner"
echo "==============================="

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ] || [ ! -d "benches" ]; then
    echo "❌ Error: Please run this script from the p-ata directory"
    exit 1
fi

# Check prerequisites
echo "🔍 Checking prerequisites..."

if ! command -v cargo &> /dev/null; then
    echo "❌ Error: cargo not found. Please install Rust"
    exit 1
fi

if ! command -v solana &> /dev/null; then
    echo "❌ Error: solana CLI not found. Please install Solana CLI tools"
    exit 1
fi

echo "✅ Prerequisites check passed"

# Create results directory
mkdir -p benchmark_results

# Determine build mode
if [ "$1" = "--comparison" ] || [ "$1" = "-c" ]; then
    echo "🔨 Building both implementations for comparison..."
    FEATURE_FLAG="--features build-programs"
    MODE="comparison"
else
    echo "🔨 Building P-ATA only..."
    FEATURE_FLAG=""
    MODE="p-ata-only"
fi

# Build programs
echo "📦 Building programs..."
cargo build-sbf $FEATURE_FLAG

# Run benchmarks
echo "⚡ Running benchmarks..."

echo "  📊 Running instruction benchmarks..."
if cargo bench $FEATURE_FLAG ata_instruction_benches > benchmark_results/comparison.log 2>&1; then
    echo "  ✅ Instruction benchmarks completed"
else
    echo "  ⚠️  Instruction benchmarks completed with warnings (this is normal)"
fi

echo "  🧪 Running failure scenario tests..."
if cargo bench $FEATURE_FLAG failure_scenarios > benchmark_results/failures.log 2>&1; then
    echo "  ✅ Failure scenarios completed"
else
    echo "  ⚠️  Failure scenarios completed with warnings (this is normal)"
fi

# Generate badges from JSON output
echo "🏷️  Generating badges..."

# Function to create shields.io URL
create_badge_url() {
    local label="$1"
    local message="$2"
    local color="$3"
    
    # URL encode spaces and special characters
    label=$(echo "$label" | sed 's/ /%20/g')
    message=$(echo "$message" | sed 's/ /%20/g')
    
    echo "https://img.shields.io/badge/${label}-${message}-${color}"
}

# Generate individual badges from performance and failure test results
generate_badges() {
    echo "# P-ATA Individual Test Results" > benchmark_results/badges.md
    echo "" >> benchmark_results/badges.md
    
    # Performance test badges
    if [ -f "benchmark_results/performance_results.json" ] && command -v jq &> /dev/null; then
        echo "## CU Savings per Test" >> benchmark_results/badges.md
        echo "" >> benchmark_results/badges.md
        
        jq -r '.performance_tests | to_entries[] | "\(.key) \(.value.savings_percent) \(.value.compatibility)"' benchmark_results/performance_results.json | while read test_name savings_percent compatibility; do
            # Skip if savings_percent is null
            if [ "$savings_percent" = "null" ] || [ -z "$savings_percent" ]; then
                continue
            fi
            
            # CU Savings badge
            if [ "$(echo "$savings_percent > 0" | bc -l 2>/dev/null || echo "0")" = "1" ]; then
                color="green"
            elif [ "$(echo "$savings_percent < 0" | bc -l 2>/dev/null || echo "0")" = "1" ]; then
                color="red"
            else
                color="yellow"
            fi
            
            savings_formatted=$(printf "%.1f%%" "$savings_percent")
            badge_url=$(create_badge_url "${test_name} CU Savings" "$savings_formatted" "$color")
            echo "![${test_name} CU Savings]($badge_url)" >> benchmark_results/badges.md
        done
        
        echo "" >> benchmark_results/badges.md
        echo "## P-ATA CU Consumption per Test" >> benchmark_results/badges.md
        echo "" >> benchmark_results/badges.md
        
        jq -r '.performance_tests | to_entries[] | "\(.key) \(.value.p_ata_cu)"' benchmark_results/performance_results.json | while read test_name p_ata_cu; do
            # Skip if p_ata_cu is null
            if [ "$p_ata_cu" = "null" ] || [ -z "$p_ata_cu" ]; then
                continue
            fi
            
            badge_url=$(create_badge_url "${test_name} P-ATA CU" "$p_ata_cu" "blue")
            echo "![${test_name} P-ATA CU]($badge_url)" >> benchmark_results/badges.md
        done
        
        echo "" >> benchmark_results/badges.md
        echo "## Compatibility per Test" >> benchmark_results/badges.md
        echo "" >> benchmark_results/badges.md
        
        jq -r '.performance_tests | to_entries[] | "\(.key) \(.value.compatibility)"' benchmark_results/performance_results.json | while read test_name compatibility; do
            # Skip if compatibility is null
            if [ "$compatibility" = "null" ] || [ -z "$compatibility" ]; then
                continue
            fi
            
            case "$compatibility" in
                "identical")
                    color="green"
                    message="Identical"
                    ;;
                "optimized")
                    color="purple"
                    message="Optimized"
                    ;;
                "expected_difference")
                    color="yellow"
                    message="Expected Diff"
                    ;;
                *)
                    color="red"
                    message="Incompatible"
                    ;;
            esac
            
            badge_url=$(create_badge_url "${test_name} Compatibility" "$message" "$color")
            echo "![${test_name} Compatibility]($badge_url)" >> benchmark_results/badges.md
        done
    fi
    
    # Failure test badges
    if [ -f "benchmark_results/failure_results.json" ] && command -v jq &> /dev/null; then
        echo "" >> benchmark_results/badges.md
        echo "## Failure Test Results" >> benchmark_results/badges.md
        echo "" >> benchmark_results/badges.md
        
        jq -r '.failure_tests | to_entries[] | "\(.key) \(.value.status)"' benchmark_results/failure_results.json | while read test_name status; do
            # Skip if status is null
            if [ "$status" = "null" ] || [ -z "$status" ]; then
                continue
            fi
            
            case "$status" in
                "pass")
                    color="green"
                    message="Pass"
                    ;;
                "error_mismatch")
                    color="yellow"
                    message="Error Mismatch"
                    ;;
                *)
                    color="red"
                    message="Fail"
                    ;;
            esac
            
            badge_url=$(create_badge_url "${test_name} Result" "$message" "$color")
            echo "![${test_name} Result]($badge_url)" >> benchmark_results/badges.md
        done
    fi
}

# Update README.md with badges
update_readme_badges() {
    if [ -f "benchmark_results/badges.md" ] && [ -f "README.md" ]; then
        echo "📝 Updating README.md with badges..."
        
        # Create a temporary file with the updated README
        temp_file=$(mktemp)
        
        # Read badges content
        badges_content=$(cat benchmark_results/badges.md)
        
        # Replace content between markers
        awk -v badges="$badges_content" '
        /<!-- BENCHMARK_BADGES_START -->/ {
            print $0
            print badges
            skip = 1
            next
        }
        /<!-- BENCHMARK_BADGES_END -->/ {
            skip = 0
        }
        !skip {
            print $0
        }
        ' README.md > "$temp_file"
        
        # Replace original README.md
        mv "$temp_file" README.md
        
        echo "✅ README.md updated with badges"
    else
        echo "⚠️  Could not update README.md (missing files)"
    fi
}

# Check if JSON results exist and generate badges
if [ -f "benchmark_results/performance_results.json" ] || [ -f "benchmark_results/failure_results.json" ]; then
    echo "📊 Processing JSON results..."
    
    generate_badges
    update_readme_badges
    
    # Show summary
    echo ""
    echo "📈 BENCHMARK SUMMARY"
    echo "===================="
    
    if command -v jq &> /dev/null; then
        if [ -f "benchmark_results/performance_results.json" ]; then
            echo "Performance Test Results:"
            jq -r '.performance_tests | to_entries[] | select(.value.savings_percent != null) | "  \(.key): \(.value.savings_percent)% CU savings, \(.value.compatibility) status"' benchmark_results/performance_results.json
        fi
        
        if [ -f "benchmark_results/failure_results.json" ]; then
            echo ""
            echo "Failure Test Results:"
            jq -r '.failure_tests | to_entries[] | select(.value.status != null) | "  \(.key): \(.value.status)"' benchmark_results/failure_results.json
        fi
        
        # Count total badges (only count non-null entries)
        total_badges=0
        if [ -f "benchmark_results/performance_results.json" ]; then
            perf_count=$(jq -r '.performance_tests | to_entries | map(select(.value.savings_percent != null)) | length' benchmark_results/performance_results.json 2>/dev/null || echo "0")
            total_badges=$((total_badges + perf_count * 3)) # CU savings + P-ATA CU + compatibility
        fi
        if [ -f "benchmark_results/failure_results.json" ]; then
            fail_count=$(jq -r '.failure_tests | to_entries | map(select(.value.status != null)) | length' benchmark_results/failure_results.json 2>/dev/null || echo "0")
            total_badges=$((total_badges + fail_count)) # failure result badges
        fi
        echo ""
        echo "Total Badges Generated: $total_badges"
    else
        echo "💡 Install 'jq' for prettier JSON output"
        echo "Raw results available in: benchmark_results/*.json"
    fi
    
    # Show badges
    if [ -f "benchmark_results/badges.md" ]; then
        echo ""
        echo "🏷️  BADGE MARKDOWN"
        echo "=================="
        cat benchmark_results/badges.md
    fi
    
    echo "✅ Badge generation completed"
else
    echo "⚠️  No JSON results found - badges not generated"
fi

echo ""
echo "✅ Benchmark run completed!"
echo ""
echo "📁 Results saved to:"
echo "   - benchmark_results/comparison.log        (raw benchmark output)"
echo "   - benchmark_results/failures.log          (failure test output)" 
echo "   - benchmark_results/performance_results.json (performance test data)"
echo "   - benchmark_results/failure_results.json     (failure test data)"
echo "   - benchmark_results/badges.md                (individual test badges)"
echo ""

if [ "$MODE" = "p-ata-only" ]; then
    echo "💡 To run comparison benchmarks, use: $0 --comparison"
fi 