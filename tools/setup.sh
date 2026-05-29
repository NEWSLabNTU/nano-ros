#!/usr/bin/env bash
# Phase 123.A.3 / 197.2 — nano-ros setup orchestrator.
#
# Derives the source submodules a target needs from the SDK index
# (nros-sdk-index.toml: platform → boards → packages+build_sources, rmw →
# packages+build_sources, --with-dev → dev_sources, --with-reference →
# [reference.*]) and provisions each via `nros setup --source`, installs rustup
# if absent, and ensures the Rust target triple is installed. The retired
# config/submodule-deps.toml is no longer read (the index is the single home).
#
# Single source of truth — `just setup` + per-platform
# `just <plat> setup` shims all `exec` this script.
#
# Usage:
#   tools/setup.sh --target=<plat>-<rmw>
#                  [--with-dev]            include dev_sources
#                  [--with-reference=<n>]  include [reference.<n>]
#                  [--rust-workspace]      write workspace Cargo.toml
#                  [--doctor]              diagnose missing deps
#                  [--list-targets]        print known plat/rmw combos
#                  [--dry-run]             print plan, fetch nothing

set -euo pipefail

# Resolve script + repo root regardless of caller's cwd.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
# Phase 197.2 — the SDK index is the single home for every source ref
# (config/submodule-deps.toml retired). `tools/setup.sh` derives a target's
# sources from the index ([board.*]/[rmw.*] `packages`+`build_sources`, plus
# `dev_sources` / [reference.*] for opt-ins) and provisions each via
# `nros setup --source <name>` (index-driven; git-submodule fallback when no
# `nros` is installed yet — operationally identical for submodule-mode sources).
INDEX="${REPO_ROOT}/nros-sdk-index.toml"

# ---------------------------------------------------------------- args

TARGET=""
PLATFORM_ONLY=""
RMW_ONLY=""
WITH_DEV=0
RUST_WORKSPACE=0
DRY_RUN=0
DOCTOR=0
LIST_TARGETS=0
SKIP_RUSTUP=0
SKIP_APT=0
declare -a WITH_REFERENCE=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --target=*)         TARGET="${1#*=}" ;;
        --target)           TARGET="$2"; shift ;;
        --platform=*)       PLATFORM_ONLY="${1#*=}" ;;
        --platform)         PLATFORM_ONLY="$2"; shift ;;
        --rmw=*)            RMW_ONLY="${1#*=}" ;;
        --rmw)              RMW_ONLY="$2"; shift ;;
        --with-dev)         WITH_DEV=1 ;;
        --with-reference=*) WITH_REFERENCE+=("${1#*=}") ;;
        --with-reference)   WITH_REFERENCE+=("$2"); shift ;;
        --rust-workspace)   RUST_WORKSPACE=1 ;;
        --skip-rustup)      SKIP_RUSTUP=1 ;;
        --skip-apt-check)   SKIP_APT=1 ;;
        --dry-run)          DRY_RUN=1 ;;
        --doctor)           DOCTOR=1 ;;
        --list-targets)     LIST_TARGETS=1 ;;
        -h|--help)
            sed -n '14,21p' "${BASH_SOURCE[0]}" | sed 's/^# //; s/^#//'
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

# Phase 197.2 — the SDK index is the single home for every source ref;
# config/submodule-deps.toml was retired. The helpers below read the index.

# Extract a single-line array `<key> = ["a", "b"]` from `[<section>]`.
# Args: <section-header> <key>   (one item per line on stdout)
index_array_for_section() {
    local section="$1" key="$2"
    awk -v section="[${section}]" -v key="${key}" '
        $0 == section            { in_sec = 1; next }
        /^\[/                    { in_sec = 0 }
        in_sec && $0 ~ "^"key"[[:space:]]*=[[:space:]]*\\[" {
            if (match($0, /\[[^]]*\]/)) {
                arr = substr($0, RSTART+1, RLENGTH-2)
                n = split(arr, parts, ",")
                for (i=1; i<=n; i++) {
                    gsub(/^[[:space:]"]+|[[:space:]"]+$/, "", parts[i])
                    sub(/[[:space:]]*#.*/, "", parts[i])
                    if (parts[i] != "") print parts[i]
                }
            }
            exit
        }
    ' "$INDEX"
}

# Board section names whose `platform = "<plat>"` matches, PLUS a board whose id
# equals <plat> (covers esp32, which the index models as a board not a platform).
index_boards_for_platform() {
    local plat="$1"
    awk -v plat="$plat" '
        /^\[board\./ { b=$0; sub(/^\[board\./,"",b); sub(/\].*/,"",b); next }
        /^\[/        { b="" }
        b != "" && $0 ~ /^platform[[:space:]]*=/ {
            v=$0; sub(/^[^=]*=[[:space:]]*/,"",v); gsub(/["[:space:]]/,"",v); sub(/#.*/,"",v)
            if (v == plat) print b
        }
        END {}
    ' "$INDEX"
    # board whose id == plat (e.g. esp32)
    grep -qE "^\[board\.${plat}\]" "$INDEX" 2>/dev/null && echo "$plat"
}

# The submodule path for a `[source.<name>]` (empty if not an index source —
# lets the caller filter out tool names like arm-none-eabi-gcc / qemu / zenohd).
index_source_submodule() {
    awk -v section="[source.$1]" '
        $0 == section { in_sec=1; next } /^\[/ { in_sec=0 }
        in_sec && $0 ~ /^submodule[[:space:]]*=/ {
            v=$0; sub(/^[^=]*=[[:space:]]*/,"",v); gsub(/["[:space:]]/,"",v); print v; exit
        }
    ' "$INDEX"
}

is_index_source() { grep -qE "^\[source\.$1\]" "$INDEX"; }

# Known platforms (board.platform values + board ids) / rmws / references.
list_known_platforms() {
    awk '/^platform[[:space:]]*=/ { v=$0; sub(/^[^=]*=[[:space:]]*/,"",v); gsub(/["[:space:]]/,"",v); sub(/#.*/,"",v); if(v!="") print v }' "$INDEX" | sort -u
}
list_known_rmws()       { grep -oE '^\[rmw\.[a-z0-9_-]+\]'       "$INDEX" | sed 's/^\[rmw\.//; s/\]//' | sort -u; }
list_known_references() { grep -oE '^\[reference\.[a-z0-9_-]+\]' "$INDEX" | sed 's/^\[reference\.//; s/\]//' | sort -u; }

# Resolve an `nros` binary for index-driven source provisioning: one on PATH,
# else the cargo-built one in the codegen submodule's target dir (the
# contributor path already builds it for codegen). Empty ⇒ none available yet
# (e.g. pre-rustup on a fresh machine); the caller falls back to git.
resolve_nros_bin() {
    if command -v nros >/dev/null 2>&1; then
        command -v nros
        return 0
    fi
    # Phase 195.D — the codegen submodule was retired; `nros` ships as a
    # prebuilt release installed by scripts/install-nros.sh into ~/.nros/bin.
    local cand="${NROS_CLI:-${NROS_HOME:-$HOME/.nros}/bin/nros}"
    if [[ -x "$cand" ]]; then
        echo "$cand"
        return 0
    fi
}

# ---------------------------------------------------------- list-targets

if (( LIST_TARGETS )); then
    echo "Known platforms:"
    list_known_platforms | sed 's/^/  /'
    echo "Known RMW backends:"
    list_known_rmws | sed 's/^/  /'
    echo "Optional references (--with-reference=<name>):"
    list_known_references | sed 's/^/  /'
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
    [[ -f "$INDEX" ]] && echo "  [ ok ] SDK index at $INDEX" \
                       || { echo "  [MISS] SDK index"; fail=1; }
    exit $fail
fi

# -------------------------------------------------------------- validate

# Mode resolution:
# (1) --target=<plat>-<rmw> sets both axes (canonical user-facing).
# (2) --platform=<plat> alone fetches the platform.<plat> + required
#     paths only — used by per-platform `just <plat> setup` shims.
# (3) --rmw=<rmw> alone fetches rmw.<rmw> + required paths only —
#     used by `just <rmw> setup` shims (cyclonedds, rmw_zenoh).
# (4) Combining --platform + --rmw without --target is equivalent
#     to --target=<plat>-<rmw>.
PLATFORM=""
RMW=""

if [[ -n "$TARGET" ]]; then
    PLATFORM="${TARGET%-*}"
    RMW="${TARGET##*-}"
    if [[ "$PLATFORM" == "$TARGET" || "$RMW" == "$TARGET" ]]; then
        err "--target='${TARGET}' is not a <platform>-<rmw> tuple."
        err "  example: posix-zenoh, freertos-xrce, threadx-dds"
        exit 2
    fi
elif [[ -n "$PLATFORM_ONLY" || -n "$RMW_ONLY" ]]; then
    PLATFORM="$PLATFORM_ONLY"
    RMW="$RMW_ONLY"
else
    err "no setup mode specified."
    err "  --target=<plat>-<rmw>           full canonical setup"
    err "  --platform=<plat>               platform paths only"
    err "  --rmw=<rmw>                     rmw paths only"
    err "  see --list-targets for known values."
    exit 2
fi

if [[ -n "$PLATFORM" ]] \
   && ! list_known_platforms | grep -qx "$PLATFORM" \
   && ! grep -qE "^\\[board\\.${PLATFORM}\\]" "$INDEX"; then
    # Accept a `[board.*].platform` value or a board id (e.g. esp32).
    err "unknown platform '${PLATFORM}'. --list-targets for known set."
    exit 2
fi
if [[ -n "$RMW" ]] && ! grep -qE "^\\[rmw\\.${RMW}\\]" "$INDEX"; then
    err "unknown rmw '${RMW}'. --list-targets for known set."
    exit 2
fi

if [[ -n "$PLATFORM" && -n "$RMW" ]]; then
    info "target = ${PLATFORM}-${RMW} (platform=${PLATFORM}, rmw=${RMW})"
elif [[ -n "$PLATFORM" ]]; then
    info "platform-only mode: ${PLATFORM}"
else
    info "rmw-only mode: ${RMW}"
fi

# --------------------------------------------------- resolve source names

# Phase 197.2 — collect the `[source.*]` NAMES this target needs from the index
# (platform → boards → packages + build_sources; rmw → packages + build_sources;
# `--with-dev` adds dev_sources; `--with-reference` adds [reference.*].sources).
# Non-source names (host tools: arm-none-eabi-gcc / qemu / zenohd / …) are
# filtered out below — only names with a `[source.*]` entry resolve to a path.
declare -a SRC_NAMES=()

if [[ -n "$PLATFORM" ]]; then
    while IFS= read -r b; do
        [[ -n "$b" ]] || continue
        while IFS= read -r n; do [[ -n "$n" ]] && SRC_NAMES+=("$n"); done \
            < <(index_array_for_section "board.${b}" "packages")
        while IFS= read -r n; do [[ -n "$n" ]] && SRC_NAMES+=("$n"); done \
            < <(index_array_for_section "board.${b}" "build_sources")
        if (( WITH_DEV )); then
            while IFS= read -r n; do [[ -n "$n" ]] && SRC_NAMES+=("$n"); done \
                < <(index_array_for_section "board.${b}" "dev_sources")
        fi
    done < <(index_boards_for_platform "$PLATFORM")
fi

if [[ -n "$RMW" ]]; then
    while IFS= read -r n; do [[ -n "$n" ]] && SRC_NAMES+=("$n"); done \
        < <(index_array_for_section "rmw.${RMW}" "packages")
    while IFS= read -r n; do [[ -n "$n" ]] && SRC_NAMES+=("$n"); done \
        < <(index_array_for_section "rmw.${RMW}" "build_sources")
    if (( WITH_DEV )); then
        while IFS= read -r n; do [[ -n "$n" ]] && SRC_NAMES+=("$n"); done \
            < <(index_array_for_section "rmw.${RMW}" "dev_sources")
    fi
fi

for ref in "${WITH_REFERENCE[@]}"; do
    if ! grep -qE "^\\[reference\\.${ref}\\]" "$INDEX"; then
        err "unknown reference '${ref}'. --list-targets for known set."
        exit 2
    fi
    while IFS= read -r n; do [[ -n "$n" ]] && SRC_NAMES+=("$n"); done \
        < <(index_array_for_section "reference.${ref}" "sources")
done

# Keep only names that are `[source.*]` (drop host tools), map to submodule path,
# dedupe by path while preserving order. Remember name↔path for provisioning.
declare -A SEEN=()
declare -a PATHS_TO_FETCH=()
declare -A PATH_SRC_NAME=()
for n in "${SRC_NAMES[@]}"; do
    is_index_source "$n" || continue
    p="$(index_source_submodule "$n")"
    [[ -n "$p" ]] || continue
    if [[ -z "${SEEN[$p]:-}" ]]; then
        SEEN[$p]=1
        PATHS_TO_FETCH+=("$p")
        PATH_SRC_NAME["$p"]="$n"
    fi
done

info "sources to fetch: ${#PATHS_TO_FETCH[@]}"
for p in "${PATHS_TO_FETCH[@]}"; do
    info "  - ${PATH_SRC_NAME[$p]} → $p"
done

# ------------------------------------------------------- fetch sources

cd "$REPO_ROOT"

# `nros` is the canonical provisioner (Phase 195.D: always installed). Fall back
# to a plain submodule update only if no `nros` is available yet (pre-install) —
# for a submodule-mode `[source.*]` that is exactly what `nros setup --source`
# runs, so the fallback is an equivalence, not a guess.
NROS_BIN="$(resolve_nros_bin)"

for p in "${PATHS_TO_FETCH[@]}"; do
    src_name="${PATH_SRC_NAME[$p]}"
    if [[ -e "$p/.git" ]] || [[ -n "$(ls -A "$p" 2>/dev/null)" ]]; then
        info "  (already populated) $p"
        continue
    fi
    if [[ -n "$NROS_BIN" ]]; then
        if (( DRY_RUN )); then
            info "[dry-run] $NROS_BIN setup --source $src_name (index [source.$src_name])"
        else
            info "fetching $p via nros setup --source $src_name ..."
            "$NROS_BIN" setup --source "$src_name" --index "$INDEX"
        fi
    elif (( DRY_RUN )); then
        info "[dry-run] git submodule update --init --depth=1 $p (index source $src_name; nros unavailable)"
    else
        info "fetching $p ... (index source $src_name; nros unavailable → equivalent git submodule update)"
        git submodule update --init --depth=1 --recursive "$p"
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

if (( SKIP_RUSTUP )); then
    info "skipping rustup install / target add (--skip-rustup)"
elif ! command -v rustup >/dev/null 2>&1; then
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
TRIPLE=""
if [[ -n "$PLATFORM" ]]; then
    TRIPLE="${RUST_TARGET_FOR_PLATFORM[$PLATFORM]:-}"
fi
if ! (( SKIP_RUSTUP )) && [[ -n "$TRIPLE" ]]; then
    if (( DRY_RUN )); then
        info "[dry-run] rustup target add $TRIPLE"
    else
        info "ensuring rustup target: $TRIPLE"
        rustup target add "$TRIPLE" >/dev/null
    fi
fi

# ---------------------------------------------------------- apt packages

if (( SKIP_APT )); then
    info "skipping apt cross-toolchain check (--skip-apt-check)"
elif [[ -n "$PLATFORM" && "${OSTYPE:-}" == linux* ]] && command -v apt-get >/dev/null 2>&1; then
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

if [[ -n "$PLATFORM" && -n "$RMW" ]]; then
    info "setup complete for target=${PLATFORM}-${RMW}"
elif [[ -n "$PLATFORM" ]]; then
    info "setup complete (platform=${PLATFORM})"
else
    info "setup complete (rmw=${RMW})"
fi
