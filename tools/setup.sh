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
# Phase 195.B — the SDK index is the SSOT for source refs. A submodule path
# that matches a `[source.*]` entry's `submodule`/`dest` is provisioned via
# `nros setup --source <name>` (index-driven) when the `nros` binary is
# available; otherwise it falls back to the operationally-identical
# `git submodule update` (submodule-mode sources resolve to the same command).
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

# Phase 195.B — emit `<path>\t<source-name>` for every `[source.*]` entry's
# `submodule` / `dest` field, so the fetch loop can recognise an index-owned
# source by path and route it through `nros setup --source`. Empty if the index
# is absent (legacy checkout) — fetch then falls back to plain submodule update.
read_index_source_paths() {
    [[ -f "$INDEX" ]] || return 0
    awk '
        /^\[source\./ { name=$0; sub(/^\[source\./,"",name); sub(/\].*/,"",name); next }
        /^\[/         { name="" }
        name != "" && $0 ~ /^(submodule|dest)[[:space:]]*=/ {
            v=$0; sub(/^[^=]*=[[:space:]]*/, "", v); gsub(/["[:space:]]/, "", v)
            if (v != "") print v "\t" name
        }
    ' "$INDEX"
}

# Resolve an `nros` binary for index-driven source provisioning: one on PATH,
# else the cargo-built one in the codegen submodule's target dir (the
# contributor path already builds it for codegen). Empty ⇒ none available yet
# (e.g. pre-rustup on a fresh machine); the caller falls back to git.
resolve_nros_bin() {
    if command -v nros >/dev/null 2>&1; then
        command -v nros
        return 0
    fi
    local t
    for t in release debug; do
        local cand="${REPO_ROOT}/packages/codegen/packages/target/${t}/nros"
        if [[ -x "$cand" ]]; then
            echo "$cand"
            return 0
        fi
    done
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

if [[ -n "$PLATFORM" ]] && ! grep -qE "^\\[platform\\.${PLATFORM}\\]" "$MANIFEST"; then
    err "unknown platform '${PLATFORM}'. --list-targets for known set."
    exit 2
fi
if [[ -n "$RMW" ]] && ! grep -qE "^\\[rmw\\.${RMW}\\]" "$MANIFEST"; then
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

# ------------------------------------------------------ resolve path set

declare -a PATHS_TO_FETCH=()

while IFS= read -r p; do
    [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
done < <(read_paths_array "required" "paths")

if [[ -n "$PLATFORM" ]]; then
    while IFS= read -r p; do
        [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
    done < <(read_paths_array "platform.${PLATFORM}" "paths")
fi

if [[ -n "$RMW" ]]; then
    while IFS= read -r p; do
        [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
    done < <(read_paths_array "rmw.${RMW}" "paths")
fi

if (( WITH_DEV )); then
    if [[ -n "$PLATFORM" ]]; then
        while IFS= read -r p; do
            [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
        done < <(read_paths_array "platform.${PLATFORM}" "dev_paths")
    fi
    if [[ -n "$RMW" ]]; then
        while IFS= read -r p; do
            [[ -n "$p" ]] && PATHS_TO_FETCH+=("$p")
        done < <(read_paths_array "rmw.${RMW}" "dev_paths")
    fi
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

# Phase 195.B — map index-owned source paths → source name; resolve an `nros`
# binary to drive their provisioning (the index is the SSOT for source refs).
declare -A IDX_SRC_NAME=()
while IFS=$'\t' read -r ipath iname; do
    [[ -n "$ipath" ]] && IDX_SRC_NAME["$ipath"]="$iname"
done < <(read_index_source_paths)
NROS_BIN="$(resolve_nros_bin)"

for p in "${PATHS_TO_FETCH[@]}"; do
    src_name="${IDX_SRC_NAME[$p]:-}"
    if [[ ! -e "$p/.git" ]] && [[ -z "$(ls -A "$p" 2>/dev/null)" ]]; then
        if [[ -n "$src_name" && -n "$NROS_BIN" ]]; then
            # Index-owned source + an nros binary → index-driven provisioning.
            if (( DRY_RUN )); then
                info "[dry-run] $NROS_BIN setup --source $src_name (index [source.$src_name])"
            else
                info "fetching $p via nros setup --source $src_name ..."
                "$NROS_BIN" setup --source "$src_name" --index "$INDEX"
            fi
        elif (( DRY_RUN )); then
            info "[dry-run] git submodule update --init --depth=1 $p${src_name:+ (index source $src_name; nros unavailable)}"
        else
            # Plain submodule update — for a submodule-mode `[source.*]` this is
            # exactly what `nros setup --source` runs, so the fallback is an
            # equivalence (used pre-rustup on a fresh machine), not a guess.
            info "fetching $p ...${src_name:+ (index source $src_name; nros unavailable → equivalent git submodule update)}"
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
