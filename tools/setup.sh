#!/usr/bin/env bash
# Phase 123.A.3 — nano-ros setup orchestrator.
#
# Reads config/submodule-deps.toml + fetches the union of paths
# required by (required, platform.<plat>, rmw.<rmw>) for the
# requested target, installs rustup if absent, and ensures the
# Rust target triple is installed.
#
# Single source of truth — `just setup` + per-platform
# `just <plat> setup` shims all `exec` this script.
#
# Usage:
#   tools/setup.sh --target=<plat>-<rmw>
#                  [--with-dev]            include dev_paths
#                  [--with-reference=<n>]  include reference.<n>
#                  [--rust-workspace]      write workspace Cargo.toml
#                  [--doctor]              diagnose missing deps
#                  [--list-targets]        print known plat/rmw combos
#                  [--dry-run]             print plan, fetch nothing

set -euo pipefail

# Resolve script + repo root regardless of caller's cwd.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
MANIFEST="${REPO_ROOT}/config/submodule-deps.toml"

# ---------------------------------------------------------------- args

TARGET=""
WITH_DEV=0
RUST_WORKSPACE=0
DRY_RUN=0
DOCTOR=0
LIST_TARGETS=0
declare -a WITH_REFERENCE=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --target=*)         TARGET="${1#*=}" ;;
        --target)           TARGET="$2"; shift ;;
        --with-dev)         WITH_DEV=1 ;;
        --with-reference=*) WITH_REFERENCE+=("${1#*=}") ;;
        --with-reference)   WITH_REFERENCE+=("$2"); shift ;;
        --rust-workspace)   RUST_WORKSPACE=1 ;;
        --dry-run)          DRY_RUN=1 ;;
        --doctor)           DOCTOR=1 ;;
        --list-targets)     LIST_TARGETS=1 ;;
        -h|--help)
            sed -n '6,21p' "${BASH_SOURCE[0]}" | sed 's/^# //; s/^#//'
            exit 0
            ;;
        *)
            echo "tools/setup.sh: unknown arg: $1" >&2
            echo "Try --help" >&2
            exit 2
            ;;
    esac
    shift
done

# -------------------------------------------------------------- helpers

err()  { echo "tools/setup.sh: $*" >&2; }
info() { echo "[setup] $*"; }

# Minimal TOML reader. Extracts the `paths = [...]` or
# `dev_paths = [...]` array from a given `[<section>]`.
# Args: <section-header> <key>
read_paths_array() {
    local section="$1" key="$2"
    awk -v section="[${section}]" -v key="${key}" '
        $0 == section            { in_sec = 1; next }
        /^\[/                    { in_sec = 0 }
        in_sec && $0 ~ "^"key" *= *\\[" {
            # Inline single-line array.
            if (match($0, /\[[^]]*\]/)) {
                arr = substr($0, RSTART+1, RLENGTH-2)
                n = split(arr, parts, ",")
                for (i=1; i<=n; i++) {
                    gsub(/^[[:space:]"]+|[[:space:]"]+$/, "", parts[i])
                    if (parts[i] != "") print parts[i]
                }
                exit
            }
            # Multi-line array.
            in_arr = 1; next
        }
        in_arr {
            if ($0 ~ /\]/) { in_arr = 0; sub(/].*/,"") }
            line = $0
            gsub(/[,]/, " ", line)
            n = split(line, parts, " ")
            for (i=1; i<=n; i++) {
                p = parts[i]
                gsub(/^[[:space:]"]+|[[:space:]"]+$/, "", p)
                if (p != "" && p != "#") print p
            }
        }
    ' "$MANIFEST"
}

list_known_sections() {
    grep -oE '^\[(rmw|platform|reference)\.[a-z0-9_-]+\]' "$MANIFEST" \
        | tr -d '[]' | sort -u
}

# ---------------------------------------------------------- list-targets

if (( LIST_TARGETS )); then
    echo "Known platforms:"
    list_known_sections | grep '^platform\.' | sed 's/^platform\./  /'
    echo "Known RMW backends:"
    list_known_sections | grep '^rmw\.' | sed 's/^rmw\./  /'
    echo "Optional references (--with-reference=<name>):"
    list_known_sections | grep '^reference\.' | sed 's/^reference\./  /'
    echo ""
    echo "Compose --target as <platform>-<rmw>, e.g. posix-zenoh."
    exit 0
fi

# -------------------------------------------------------------- doctor

if (( DOCTOR )); then
    fail=0
    for cmd in git rustup cargo cmake; do
        if command -v "$cmd" >/dev/null 2>&1; then
            printf "  [ ok ] %s -> %s\n" "$cmd" "$(command -v "$cmd")"
        else
            printf "  [MISS] %s (not on PATH)\n" "$cmd"
            fail=1
        fi
    done
    [[ -f "$MANIFEST" ]] && echo "  [ ok ] manifest at $MANIFEST" \
                          || { echo "  [MISS] manifest"; fail=1; }
    exit $fail
fi

# -------------------------------------------------------------- validate

if [[ -z "$TARGET" ]]; then
    err "no --target specified."
    err "  example: --target=posix-zenoh"
    err "  see --list-targets for available combos."
    exit 2
fi

# Split <plat>-<rmw>. RMW is the last hyphen-delimited token,
# platform is everything before. Lets the platform name itself
# contain hyphens (none today, but future-proof).
PLATFORM="${TARGET%-*}"
RMW="${TARGET##*-}"

if [[ "$PLATFORM" == "$TARGET" || "$RMW" == "$TARGET" ]]; then
    err "--target='${TARGET}' is not a <platform>-<rmw> tuple."
    err "  example: posix-zenoh, freertos-xrce, threadx-dds"
    exit 2
fi

# Validate sections exist in manifest.
if ! grep -qE "^\\[platform\\.${PLATFORM}\\]" "$MANIFEST"; then
    err "unknown platform '${PLATFORM}'. --list-targets for known set."
    exit 2
fi
if ! grep -qE "^\\[rmw\\.${RMW}\\]" "$MANIFEST"; then
    err "unknown rmw '${RMW}'. --list-targets for known set."
    exit 2
fi

info "target = ${TARGET} (platform=${PLATFORM}, rmw=${RMW})"

# ------------------------------------------------------ resolve path set

declare -a PATHS_TO_FETCH=()

while IFS= read -r p; do
    [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
done < <(read_paths_array "required" "paths")

while IFS= read -r p; do
    [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
done < <(read_paths_array "platform.${PLATFORM}" "paths")

while IFS= read -r p; do
    [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
done < <(read_paths_array "rmw.${RMW}" "paths")

if (( WITH_DEV )); then
    while IFS= read -r p; do
        [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
    done < <(read_paths_array "platform.${PLATFORM}" "dev_paths")
    while IFS= read -r p; do
        [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
    done < <(read_paths_array "rmw.${RMW}" "dev_paths")
fi

for ref in "${WITH_REFERENCE[@]}"; do
    if ! grep -qE "^\\[reference\\.${ref}\\]" "$MANIFEST"; then
        err "unknown reference '${ref}'. --list-targets for known set."
        exit 2
    fi
    while IFS= read -r p; do
        [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
    done < <(read_paths_array "reference.${ref}" "paths")
done

# Dedupe while preserving order.
declare -A SEEN=()
declare -a UNIQ=()
for p in "${PATHS_TO_FETCH[@]}"; do
    if [[ -z "${SEEN[$p]:-}" ]]; then
        SEEN[$p]=1
        UNIQ+=("$p")
    fi
done
PATHS_TO_FETCH=("${UNIQ[@]}")

info "submodules to fetch: ${#PATHS_TO_FETCH[@]}"
for p in "${PATHS_TO_FETCH[@]}"; do
    info "  - $p"
done

# ------------------------------------------------------- fetch submodules

cd "$REPO_ROOT"

for p in "${PATHS_TO_FETCH[@]}"; do
    if [[ ! -e "$p/.git" ]] && [[ -z "$(ls -A "$p" 2>/dev/null)" ]]; then
        if (( DRY_RUN )); then
            info "[dry-run] git submodule update --init --depth=1 $p"
        else
            info "fetching $p ..."
            git submodule update --init --depth=1 --recursive "$p"
        fi
    else
        info "  (already populated) $p"
    fi
done

# ---------------------------------------------------------- rustup setup

# Map nano-ros platform -> default Rust target triple. POSIX = host.
declare -A RUST_TARGET_FOR_PLATFORM=(
    [posix]=""
    [freertos]="thumbv7m-none-eabi"
    [nuttx]="thumbv7em-none-eabihf"
    [threadx]="thumbv7em-none-eabihf"
    [zephyr]="thumbv7em-none-eabihf"
    [bare-metal]="thumbv7m-none-eabi"
    [esp32]="riscv32imc-unknown-none-elf"
)

if ! command -v rustup >/dev/null 2>&1; then
    if (( DRY_RUN )); then
        info "[dry-run] would install rustup via https://sh.rustup.rs"
    else
        info "installing rustup (no toolchain by default)..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
            | sh -s -- -y --default-toolchain none --profile minimal
        # shellcheck source=/dev/null
        source "$HOME/.cargo/env"
    fi
fi

# Use the workspace's pinned toolchain (rust-toolchain.toml) — rustup
# picks it up automatically when run inside the repo. Add the target
# triple if the platform needs cross-compilation.
TRIPLE="${RUST_TARGET_FOR_PLATFORM[$PLATFORM]:-}"
if [[ -n "$TRIPLE" ]]; then
    if (( DRY_RUN )); then
        info "[dry-run] rustup target add $TRIPLE"
    else
        info "ensuring rustup target: $TRIPLE"
        rustup target add "$TRIPLE" >/dev/null
    fi
fi

# ---------------------------------------------------------- apt packages

if [[ "${OSTYPE:-}" == linux* ]] && command -v apt-get >/dev/null 2>&1; then
    declare -A APT_FOR_PLATFORM=(
        [freertos]="gcc-arm-none-eabi"
        [nuttx]="gcc-arm-none-eabi kconfig-frontends"
        [threadx]="gcc-arm-none-eabi"
        [zephyr]="gcc-arm-none-eabi"
        [bare-metal]="gcc-arm-none-eabi"
    )
    pkgs="${APT_FOR_PLATFORM[$PLATFORM]:-}"
    if [[ -n "$pkgs" ]]; then
        missing=()
        for p in $pkgs; do
            if ! dpkg -s "$p" >/dev/null 2>&1; then
                missing+=("$p")
            fi
        done
        if (( ${#missing[@]} > 0 )); then
            info "missing apt packages: ${missing[*]}"
            info "  install with: sudo apt install ${missing[*]}"
            info "  (tools/setup.sh never runs sudo automatically)"
        fi
    fi
fi

# ------------------------------------------------------- rust-workspace

if (( RUST_WORKSPACE )); then
    info "--rust-workspace requested. Cargo.toml emission deferred to a"
    info "  follow-up (A.10). For now, edit your workspace Cargo.toml"
    info "  manually; see docs/roadmap/phase-123-build-and-api-revision.md"
    info "  → 'User workflows' for the recommended template."
fi

info "setup complete for target=${TARGET}"
