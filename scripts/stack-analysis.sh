#!/usr/bin/env bash
# Stack Usage Analysis for nano-ros embedded examples
#
# Builds an example with nightly + -Z emit-stack-sizes, then parses
# the .stack_sizes ELF section via llvm-readobj to show per-function
# stack usage.
#
# Usage: ./scripts/stack-analysis.sh [example-dir] [--top N] [--filter PATTERN]
#
# Defaults to examples/qemu/rs-wcet-bench if no example-dir given.
# Paths are resolved relative to the repository root (auto-detected).

set -euo pipefail

# --- Resolve repository root (CWD-independent) ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Argument parsing ---
EXAMPLE_DIR=""
TOP=30
FILTER=""

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
        -h|--help)
            echo "Usage: $0 [example-dir] [--top N] [--filter PATTERN]"
            echo ""
            echo "  example-dir   Path to the example to analyze (default: examples/qemu/rs-wcet-bench)"
            echo "                Relative paths are resolved from the repository root."
            echo "  --top N       Show top N functions by stack size (default: 30)"
            echo "  --filter PAT  Only show functions matching grep pattern (applied after demangling)"
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

# Resolve example dir: if relative, resolve from repo root
EXAMPLE_DIR="${EXAMPLE_DIR:-examples/qemu/rs-wcet-bench}"
if [[ "$EXAMPLE_DIR" != /* ]]; then
    EXAMPLE_DIR="$REPO_ROOT/$EXAMPLE_DIR"
fi

if [[ ! -d "$EXAMPLE_DIR" ]]; then
    echo "Error: example directory '$EXAMPLE_DIR' does not exist" >&2
    exit 1
fi

if [[ ! -f "$EXAMPLE_DIR/Cargo.toml" ]]; then
    echo "Error: no Cargo.toml found in '$EXAMPLE_DIR'" >&2
    exit 1
fi

# --- Locate llvm-readobj from rustup nightly sysroot ---
SYSROOT="$(rustc +nightly --print sysroot 2>/dev/null)" || {
    echo "Error: nightly toolchain not found. Install it with: rustup toolchain install nightly" >&2
    exit 1
}
HOST="$(rustc +nightly -vV | grep '^host:' | cut -d' ' -f2)"
LLVM_READOBJ="$SYSROOT/lib/rustlib/$HOST/bin/llvm-readobj"

if [[ ! -x "$LLVM_READOBJ" ]]; then
    echo "Error: llvm-readobj not found at $LLVM_READOBJ" >&2
    echo "Install it with: rustup +nightly component add llvm-tools" >&2
    exit 1
fi

# --- Locate demangler ---
DEMANGLER=""
if command -v rustfilt &>/dev/null; then
    DEMANGLER="rustfilt"
elif command -v c++filt &>/dev/null; then
    DEMANGLER="c++filt"
fi

demangle() {
    if [[ -n "$DEMANGLER" ]]; then
        "$DEMANGLER"
    else
        cat
    fi
}

# --- Detect target triple from .cargo/config.toml ---
TARGET=""
CONFIG_FILE="$EXAMPLE_DIR/.cargo/config.toml"
if [[ -f "$CONFIG_FILE" ]]; then
    TARGET="$(grep -E '^\s*target\s*=' "$CONFIG_FILE" | head -1 | sed 's/.*=\s*"\(.*\)".*/\1/')"
fi

if [[ -z "$TARGET" ]]; then
    echo "Error: could not detect target triple from $CONFIG_FILE" >&2
    echo "Ensure the file has: [build] target = \"...\"" >&2
    exit 1
fi

# --- Detect binary name from Cargo.toml ---
BIN_NAME="$(grep -A2 '^\[\[bin\]\]' "$EXAMPLE_DIR/Cargo.toml" | grep 'name' | head -1 | sed 's/.*=\s*"\(.*\)".*/\1/')"
if [[ -z "$BIN_NAME" ]]; then
    # Fall back to package name
    BIN_NAME="$(grep '^name' "$EXAMPLE_DIR/Cargo.toml" | head -1 | sed 's/.*=\s*"\(.*\)".*/\1/')"
fi

if [[ -z "$BIN_NAME" ]]; then
    echo "Error: could not determine binary name from Cargo.toml" >&2
    exit 1
fi

# Display path relative to repo root for readability
DISPLAY_DIR="${EXAMPLE_DIR#"$REPO_ROOT/"}"

echo "=== Stack Usage Analysis ==="
echo "Example: $DISPLAY_DIR"
echo "Target:  $TARGET"
echo "Binary:  $BIN_NAME"
echo ""

# --- Build with -Z emit-stack-sizes ---
echo "Building with -Z emit-stack-sizes..."

# When RUSTFLAGS env is set, cargo ignores target-specific rustflags from
# .cargo/config.toml. We must merge them manually so the linker script
# (-Tlink.x etc.) is still passed.
EXISTING_FLAGS=""
if [[ -f "$CONFIG_FILE" ]]; then
    TARGET_SECTION="[target.${TARGET}]"
    EXISTING_FLAGS="$(awk -v section="$TARGET_SECTION" '
        BEGIN { in_target=0; in_flags=0 }
        $0 == section { in_target=1; next }
        in_target && /^\[/ { in_target=0 }
        in_target && /^rustflags/ { in_flags=1 }
        in_flags && /\]/ { in_flags=0 }
        in_flags && /^[[:space:]]*"/ { gsub(/[",]/, ""); printf "%s ", $0 }
    ' "$CONFIG_FILE" | tr -s ' ')"
fi

(
    cd "$EXAMPLE_DIR"
    RUSTFLAGS="-Z emit-stack-sizes ${EXISTING_FLAGS}" cargo +nightly build --release 2>&1 \
        | grep -v '^\s*Compiling\|^\s*Finished\|^\s*Downloaded\|^\s*Downloading' || true
)
echo ""

# --- Find the ELF binary ---
ELF_PATH="$EXAMPLE_DIR/target/$TARGET/release/$BIN_NAME"

if [[ ! -f "$ELF_PATH" ]]; then
    echo "Error: ELF binary not found at $ELF_PATH" >&2
    exit 1
fi

echo "Analyzing: ${ELF_PATH#"$REPO_ROOT/"}"
echo ""

# --- Parse stack sizes ---
RAW_OUTPUT="$("$LLVM_READOBJ" --stack-sizes "$ELF_PATH" 2>/dev/null)"

# llvm-readobj --stack-sizes output format:
#   Entry {
#     Functions: [sym1, sym2]
#     Size: 0x1A0
#   }
# Sizes are hex. Multiple functions may share one entry (same stack frame).
# We emit one line per function.
PARSED="$(echo "$RAW_OUTPUT" | awk '
    /Functions:/ {
        line = $0
        # Strip "Functions: [" prefix and trailing "]"
        sub(/.*Functions: *\[/, "", line)
        sub(/\] *$/, "", line)
        # Split on ", " for multiple functions
        n = split(line, funcs, ", ")
        func_count = n
    }
    /^[[:space:]]+Size: 0x/ {
        # Convert hex to decimal (only match indented Size inside entries)
        hex = $2
        gsub(/^0[xX]/, "", hex)
        cmd = "printf \"%d\" 0x" hex
        cmd | getline size
        close(cmd)
        for (i = 1; i <= func_count; i++) {
            if (funcs[i] != "") {
                printf "%d\t%s\n", size, funcs[i]
            }
        }
        func_count = 0
    }
')"

if [[ -z "$PARSED" ]]; then
    echo "No stack size information found."
    echo "This may mean the .stack_sizes section was not emitted."
    exit 1
fi

# Demangle function names
PARSED="$(echo "$PARSED" | while IFS=$'\t' read -r size func; do
    demangled="$(echo "$func" | demangle)"
    printf "%d\t%s\n" "$size" "$demangled"
done)"

# Apply filter if given (matches against demangled names)
if [[ -n "$FILTER" ]]; then
    PARSED="$(echo "$PARSED" | grep -i "$FILTER" || true)"
    if [[ -z "$PARSED" ]]; then
        echo "No functions matching filter '$FILTER'"
        exit 0
    fi
fi

# Sort by stack size descending, take top N
SORTED="$(echo "$PARSED" | sort -t$'\t' -k1 -rn | head -n "$TOP")"

# --- Display formatted table ---
printf "%-8s %s\n" "STACK" "FUNCTION"
printf "%-8s %s\n" "-----" "--------"
echo "$SORTED" | while IFS=$'\t' read -r size func; do
    printf "%-8s %s\n" "$size" "$func"
done
echo ""

# --- Summary ---
TOTAL="$(echo "$PARSED" | wc -l)"
MAX_STACK="$(echo "$PARSED" | sort -t$'\t' -k1 -rn | head -1 | cut -f1)"
LARGE="$(echo "$PARSED" | awk -F'\t' '$1 > 256' | wc -l)"

echo "Summary: $TOTAL functions, max stack = $MAX_STACK bytes"
echo "         $LARGE functions with stack > 256 bytes"
