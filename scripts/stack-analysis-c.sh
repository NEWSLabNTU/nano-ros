#!/usr/bin/env bash
# Stack Usage Analysis for nros C examples
#
# Builds a C example with gcc -fstack-usage, then parses the .su files
# to show per-function stack usage.
#
# Usage: ./scripts/stack-analysis-c.sh [example-dir] [--top N] [--filter PATTERN]
#
# Defaults to examples/native/c-talker if no example-dir given.
# Paths are resolved relative to the repository root (auto-detected).

set -euo pipefail

# --- Resolve repository root (CWD-independent) ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Argument parsing ---
EXAMPLE_DIR=""
TOP=30
FILTER=""
EXCLUDE=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --top)
            TOP="$2"
            shift 2
            ;;
        --filter)
            FILTER="$2"
            shift 2
            ;;
        --exclude)
            EXCLUDE="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [example-dir] [--top N] [--filter PATTERN]"
            echo ""
            echo "  example-dir   Path to a CMake-based C example (default: examples/native/c-talker)"
            echo "                Relative paths are resolved from the repository root."
            echo "  --top N       Show top N functions by stack size (default: 30)"
            echo "  --filter PAT  Only show functions matching grep pattern"
            echo "  --exclude PAT Exclude functions matching grep -E pattern"
            exit 0
            ;;
        -*)
            echo "Error: unknown option '$1'" >&2
            exit 1
            ;;
        *)
            EXAMPLE_DIR="$1"
            shift
            ;;
    esac
done

# Resolve example dir
EXAMPLE_DIR="${EXAMPLE_DIR:-examples/native/c-talker}"
if [[ "$EXAMPLE_DIR" != /* ]]; then
    EXAMPLE_DIR="$REPO_ROOT/$EXAMPLE_DIR"
fi

if [[ ! -d "$EXAMPLE_DIR" ]]; then
    echo "Error: example directory '$EXAMPLE_DIR' does not exist" >&2
    exit 1
fi

if [[ ! -f "$EXAMPLE_DIR/CMakeLists.txt" ]]; then
    echo "Error: no CMakeLists.txt found in '$EXAMPLE_DIR'" >&2
    exit 1
fi

# Check for cmake and gcc
if ! command -v cmake &>/dev/null; then
    echo "Error: cmake not found. Please install cmake." >&2
    exit 1
fi

DISPLAY_DIR="${EXAMPLE_DIR#"$REPO_ROOT/"}"
BUILD_DIR="$EXAMPLE_DIR/build"

echo "=== Stack Usage Analysis (C) ==="
echo "Example: $DISPLAY_DIR"
echo ""

# --- Build with -fstack-usage ---
echo "Building with -fstack-usage..."

mkdir -p "$BUILD_DIR"
# Remove stale CMake cache (directory layout may have changed)
rm -f "$BUILD_DIR/CMakeCache.txt"
(
    cd "$BUILD_DIR"
    cmake .. -DNANO_ROS_ROOT="$REPO_ROOT" \
        -DCMAKE_C_FLAGS="-fstack-usage" \
        -DCMAKE_BUILD_TYPE=Release \
        > /dev/null 2>&1
    cmake --build . --clean-first > /dev/null 2>&1
)
echo ""

# --- Find and parse .su files ---
# GCC .su format: <file>:<line>:<col>:<function>\t<size>\t<type>
# where type is "static", "dynamic", or "bounded"
SU_FILES="$(find "$BUILD_DIR" -name '*.su' 2>/dev/null)"

if [[ -z "$SU_FILES" ]]; then
    echo "No .su files found in $BUILD_DIR"
    echo "Ensure the build uses gcc (not clang) — clang does not support -fstack-usage."
    exit 1
fi

echo "Analyzing .su files in: ${BUILD_DIR#"$REPO_ROOT/"}"
echo ""

# Parse all .su files into "size\tfunction\ttype\tfile:line" format
TMPFILE="$(mktemp)"
trap 'rm -f "$TMPFILE"' EXIT

while IFS= read -r su_file; do
    while IFS=$'\t' read -r location size type; do
        # Extract function name from location (file:line:col:function)
        func="$(echo "$location" | rev | cut -d: -f1 | rev)"
        # Extract file:line (strip column and function)
        file_line="$(echo "$location" | cut -d: -f1-2)"
        # Make file path relative
        file_line="${file_line#"$REPO_ROOT/"}"
        printf "%s\t%s\t%s\t%s\n" "$size" "$func" "$type" "$file_line" >> "$TMPFILE"
    done < "$su_file"
done <<< "$SU_FILES"

if [[ ! -s "$TMPFILE" ]]; then
    echo "No stack usage data found in .su files."
    exit 1
fi

# Apply filter if given
if [[ -n "$FILTER" ]]; then
    FILTERED="$(mktemp)"
    grep -i "$FILTER" "$TMPFILE" > "$FILTERED" || true
    mv "$FILTERED" "$TMPFILE"
    if [[ ! -s "$TMPFILE" ]]; then
        echo "No functions matching filter '$FILTER'"
        exit 0
    fi
fi

# Apply exclude if given (removes matching functions)
if [[ -n "$EXCLUDE" ]]; then
    FILTERED="$(mktemp)"
    grep -v -E "$EXCLUDE" "$TMPFILE" > "$FILTERED" || true
    mv "$FILTERED" "$TMPFILE"
    if [[ ! -s "$TMPFILE" ]]; then
        echo "No functions remaining after exclude '$EXCLUDE'"
        exit 0
    fi
fi

# Sort by stack size descending
sort -t$'\t' -k1 -rn "$TMPFILE" -o "$TMPFILE"

# --- Display formatted table ---
printf "%-8s %-8s %-30s %s\n" "STACK" "TYPE" "FUNCTION" "LOCATION"
printf "%-8s %-8s %-30s %s\n" "-----" "----" "--------" "--------"
head -n "$TOP" "$TMPFILE" | while IFS=$'\t' read -r size func type file_line; do
    printf "%-8s %-8s %-30s %s\n" "$size" "$type" "$func" "$file_line"
done
echo ""

# --- Summary ---
TOTAL="$(wc -l < "$TMPFILE")"
MAX_STACK="$(head -1 "$TMPFILE" | cut -f1)"
LARGE="$(awk -F'\t' '$1 > 256' "$TMPFILE" | wc -l)"
DYNAMIC="$(awk -F'\t' '$3 == "dynamic"' "$TMPFILE" | wc -l)"

echo "Summary: $TOTAL functions, max stack = $MAX_STACK bytes"
echo "         $LARGE functions with stack > 256 bytes"
if [[ "$DYNAMIC" -gt 0 ]]; then
    echo "         $DYNAMIC functions with dynamic (unbounded) stack usage"
fi
