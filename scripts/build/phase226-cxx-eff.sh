#!/usr/bin/env bash
# Opt-in Phase 226 diagnostic runner for C/C++ fixture build efficiency.
#
# Runs selected manifest CMake fixture cells, records sccache stats before and
# after, and summarizes real Cargo/CMake work from the captured logs.
set -euo pipefail

usage() {
    cat <<'EOF'
usage: scripts/build/phase226-cxx-eff.sh [options]

Options:
  --platform <name>       Fixture platform (default: native)
  --lang <c|cpp|all>      Fixture language (default: all)
  --rmw <name>            Restrict to one RMW (for example zenoh or xrce)
  --role <name>           Restrict by fixture role directory basename
  --cell <dir[:sub]>      Restrict to an explicit cell; may be repeated
  --limit <n>             Run at most n matching cells
  --log-dir <dir>         Output directory (default: tmp/phase226-cxx-eff/<stamp>)
  --dry-run               List selected cells without building
  --list                  Alias for --dry-run
  -h, --help              Show this help

Examples:
  scripts/build/phase226-cxx-eff.sh --lang c --rmw zenoh --role talker
  scripts/build/phase226-cxx-eff.sh --lang cpp --rmw xrce --limit 2
  scripts/build/phase226-cxx-eff.sh --cell examples/native/c/talker:build-zenoh

Notes:
  This script is diagnostic only. It is not used by normal builds.
  Each cell log starts with the effective Cargo/Corrosion wrapper env, and
  each cell also writes a small CMake cache snapshot next to the build log.
  For native cells, if NROS_CMAKE_EXTRA_DEFS is unset, the script prepares the
  C codegen tool and uses the same Release/codegen-off defaults as the native
  fixture recipe for zenoh/xrce manifest cells.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

platform="native"
lang="all"
rmw=""
role=""
limit=""
dry_run=0
log_dir=""
cells=()

while [ "$#" -gt 0 ]; do
    case "$1" in
        --platform)
            platform="${2:?missing value for --platform}"
            shift 2
            ;;
        --lang)
            lang="${2:?missing value for --lang}"
            shift 2
            ;;
        --rmw)
            rmw="${2:?missing value for --rmw}"
            shift 2
            ;;
        --role)
            role="${2:?missing value for --role}"
            shift 2
            ;;
        --cell)
            cells+=("${2:?missing value for --cell}")
            shift 2
            ;;
        --limit)
            limit="${2:?missing value for --limit}"
            shift 2
            ;;
        --log-dir)
            log_dir="${2:?missing value for --log-dir}"
            shift 2
            ;;
        --dry-run|--list)
            dry_run=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

case "$lang" in
    c|cpp|all) ;;
    *)
        echo "invalid --lang '$lang'; expected c, cpp, or all" >&2
        exit 2
        ;;
esac

if [ -n "$limit" ] && ! [[ "$limit" =~ ^[0-9]+$ ]]; then
    echo "invalid --limit '$limit'; expected a non-negative integer" >&2
    exit 2
fi

stamp="$(date +%Y%m%d-%H%M%S)"
log_dir="${log_dir:-tmp/phase226-cxx-eff/$stamp}"

# shellcheck source=scripts/build/cmake-incremental.sh
source scripts/build/cmake-incremental.sh

langs=()
if [ "$lang" = "all" ]; then
    langs=(c cpp)
else
    langs=("$lang")
fi

manifest_rows() {
    local one_lang="$1"
    python3 scripts/build/fixtures-manifest.py list \
        --platform "$platform" --lang "$one_lang" ${rmw:+--rmw "$rmw"}
}

cell_matches_explicit_filter() {
    local dir="$1"
    local sub="$2"
    local cell

    [ "${#cells[@]}" -gt 0 ] || return 0
    for cell in "${cells[@]}"; do
        case "$cell" in
            *:*)
                [ "$cell" = "$dir:$sub" ] && return 0
                ;;
            *)
                [ "$cell" = "$dir" ] && return 0
                ;;
        esac
    done
    return 1
}

selected_rows_file="$(mktemp "${TMPDIR:-/tmp}/nros-phase226-cxx.XXXXXX")"
trap 'rm -f "$selected_rows_file"' EXIT

selected=0
for one_lang in "${langs[@]}"; do
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        IFS=$'\x1f' read -r dir sub defs target <<< "$line"
        [ -n "${dir:-}" ] && [ -n "${sub:-}" ] || continue
        if [ -n "$role" ] && [ "$(basename "$dir")" != "$role" ]; then
            continue
        fi
        if ! cell_matches_explicit_filter "$dir" "$sub"; then
            continue
        fi
        if [ -n "$limit" ] && [ "$selected" -ge "$limit" ]; then
            continue
        fi
        printf '%s\t%s\n' "$one_lang" "$line" >> "$selected_rows_file"
        selected=$((selected + 1))
    done < <(manifest_rows "$one_lang")
done

if [ "$selected" -eq 0 ]; then
    echo "No C/C++ fixture cells matched the requested filters." >&2
    exit 1
fi

echo "Selected $selected fixture cell(s):"
while IFS=$'\t' read -r one_lang line; do
    IFS=$'\x1f' read -r dir sub defs target <<< "$line"
    printf '  %s %s/%s %s%s\n' "$one_lang" "$dir" "$sub" "$defs" "${target:+ target=$target}"
done < "$selected_rows_file"

if [ "$dry_run" = "1" ]; then
    exit 0
fi

mkdir -p "$log_dir/cells"

if [ "$platform" = "native" ] && [ -z "${NROS_CMAKE_EXTRA_DEFS:-}" ]; then
    # shellcheck source=scripts/build/cargo.sh
    source scripts/build/cargo.sh
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
    export NROS_CMAKE_EXTRA_DEFS="-DCMAKE_BUILD_TYPE=Release -DNANO_ROS_BUILD_CODEGEN=OFF -D_NANO_ROS_CODEGEN_TOOL=${codegen_tool} -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=${repo_root}/scripts/cyclonedds/msg_to_cyclone_idl.py"
fi

export CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-never}"
export CMAKE_COLOR_DIAGNOSTICS="${CMAKE_COLOR_DIAGNOSTICS:-OFF}"
export CARGO_LOG="${CARGO_LOG:-cargo::core::compiler::fingerprint=info}"

sccache_available=0
if command -v sccache >/dev/null 2>&1; then
    sccache_available=1
    sccache --show-stats > "$log_dir/sccache-before.txt" 2>&1 || true
else
    printf 'sccache not found on PATH\n' > "$log_dir/sccache-before.txt"
fi

summary_file="$log_dir/summary.txt"
: > "$summary_file"
printf 'Phase 226 C/C++ efficiency diagnostic\n' >> "$summary_file"
printf 'log_dir=%s\nplatform=%s\nlang=%s\nrmw=%s\nrole=%s\n' \
    "$log_dir" "$platform" "$lang" "${rmw:-<all>}" "${role:-<all>}" >> "$summary_file"
printf 'CARGO_LOG=%s\n\n' "$CARGO_LOG" >> "$summary_file"

print_env_snapshot() {
    printf 'effective_env:\n'
    printf '  RUSTC_WRAPPER=%s\n' "${RUSTC_WRAPPER:-<unset>}"
    printf '  CARGO_TARGET_DIR=%s\n' "${CARGO_TARGET_DIR:-<unset>}"
    printf '  CARGO_BUILD_TARGET=%s\n' "${CARGO_BUILD_TARGET:-<unset>}"
    printf '  CARGO_HOME=%s\n' "${CARGO_HOME:-<unset>}"
    printf '  RUSTUP_TOOLCHAIN=%s\n' "${RUSTUP_TOOLCHAIN:-<unset>}"
    printf '  RUSTFLAGS=%s\n' "${RUSTFLAGS:-<unset>}"
    printf '  NROS_JOBSERVER=%s\n' "${NROS_JOBSERVER:-<unset>}"
    printf '  MAKEFLAGS=%s\n' "${MAKEFLAGS:-<unset>}"
    printf '  CMAKE_BUILD_PARALLEL_LEVEL=%s\n' "${CMAKE_BUILD_PARALLEL_LEVEL:-<unset>}"
    printf '  CMAKE_C_COMPILER_LAUNCHER=%s\n' "${CMAKE_C_COMPILER_LAUNCHER:-<unset>}"
    printf '  CMAKE_CXX_COMPILER_LAUNCHER=%s\n' "${CMAKE_CXX_COMPILER_LAUNCHER:-<unset>}"
    printf '  NROS_CMAKE_EXTRA_DEFS=%s\n' "${NROS_CMAKE_EXTRA_DEFS:-<unset>}"
}

write_cmake_cache_snapshot() {
    local cache="$1"
    local out="$2"

    if [ ! -f "$cache" ]; then
        printf 'CMakeCache.txt not found: %s\n' "$cache" > "$out"
        return
    fi

    grep -E \
        '^(Rust_CARGO_TARGET|CARGO_TARGET_DIR|CARGO_BUILD_TARGET|CMAKE_TOOLCHAIN_FILE|CMAKE_BUILD_TYPE|CMAKE_C_COMPILER_LAUNCHER|CMAKE_CXX_COMPILER_LAUNCHER|NANO_ROS_PLATFORM|NANO_ROS_RMW|NROS_RMW|NANO_ROS_BUILD_CODEGEN)' \
        "$cache" > "$out" || printf 'No matching cache variables found in %s\n' "$cache" > "$out"
}

run_cell() {
    local index="$1"
    local one_lang="$2"
    local line="$3"
    local dir sub defs target build_dir safe log cache_snapshot status

    IFS=$'\x1f' read -r dir sub defs target <<< "$line"
    build_dir="$dir/$sub"
    safe="$(printf '%03d-%s-%s-%s' "$index" "$one_lang" "$dir" "$sub" | tr '/ ' '__')"
    log="$log_dir/cells/$safe.log"
    cache_snapshot="$log_dir/cells/$safe.cmake-cache.txt"

    set +e
    {
        printf 'cell_lang=%s\ncell_dir=%s\ncell_build_dir=%s\ncell_defs=%s\ncell_target=%s\n\n' \
            "$one_lang" "$dir" "$build_dir" "$defs" "${target:-<default>}"
        print_env_snapshot
        printf '\n'
        # shellcheck disable=SC2086
        nros_cmake_configure_if_needed "$dir" "$build_dir" $defs ${NROS_CMAKE_EXTRA_DEFS:-}
        args=(--build "$build_dir")
        [ -n "${target:-}" ] && args+=(--target "$target")
        cmake "${args[@]}"
    } > "$log" 2>&1
    status=$?
    set -e

    write_cmake_cache_snapshot "$build_dir/CMakeCache.txt" "$cache_snapshot"
    summarize_cell "$one_lang" "$dir" "$sub" "$target" "$status" "$log" "$cache_snapshot" >> "$summary_file"
    return "$status"
}

count_pattern() {
    local pattern="$1"
    local file="$2"
    grep -Ec "$pattern" "$file" || true
}

summarize_cell() {
    local one_lang="$1"
    local dir="$2"
    local sub="$3"
    local target="$4"
    local status="$5"
    local log="$6"
    local cache_snapshot="$7"

    printf 'cell %s %s/%s%s\n' "$one_lang" "$dir" "$sub" "${target:+ target=$target}"
    printf '  status: %s\n' "$status"
    printf '  log: %s\n' "$log"
    printf '  cmake cache snapshot: %s\n' "$cache_snapshot"
    printf '  Compiling nros-c: %s\n' "$(count_pattern '^[[:space:]]*Compiling nros-c v|[[:space:]]Compiling nros-c v' "$log")"
    printf '  Compiling nros-cpp: %s\n' "$(count_pattern '^[[:space:]]*Compiling nros-cpp v|[[:space:]]Compiling nros-cpp v' "$log")"
    printf '  C object builds: %s\n' "$(count_pattern 'Building C object' "$log")"
    printf '  CXX object builds: %s\n' "$(count_pattern 'Building CXX object' "$log")"
    printf '  link steps: %s\n' "$(count_pattern 'Linking (C|CXX)' "$log")"
    printf '  cargo fingerprint lines: %s\n\n' "$(count_pattern 'fingerprint' "$log")"
}

failed=0
index=0
while IFS=$'\t' read -r one_lang line; do
    index=$((index + 1))
    IFS=$'\x1f' read -r dir sub _defs _target <<< "$line"
    echo "Running [$index/$selected] $one_lang $dir/$sub"
    if ! run_cell "$index" "$one_lang" "$line"; then
        failed=1
        echo "  failed; see $log_dir/cells"
    fi
done < "$selected_rows_file"

if [ "$sccache_available" = "1" ]; then
    sccache --show-stats > "$log_dir/sccache-after.txt" 2>&1 || true
else
    printf 'sccache not found on PATH\n' > "$log_dir/sccache-after.txt"
fi

all_logs="$log_dir/all-cell-logs.txt"
find "$log_dir/cells" -type f -name '*.log' -print0 | sort -z | xargs -0 cat > "$all_logs" 2>/dev/null || :

{
    printf 'aggregate\n'
    printf '  Compiling nros-c: %s\n' "$(count_pattern '^[[:space:]]*Compiling nros-c v|[[:space:]]Compiling nros-c v' "$all_logs")"
    printf '  Compiling nros-cpp: %s\n' "$(count_pattern '^[[:space:]]*Compiling nros-cpp v|[[:space:]]Compiling nros-cpp v' "$all_logs")"
    printf '  C object builds: %s\n' "$(count_pattern 'Building C object' "$all_logs")"
    printf '  CXX object builds: %s\n' "$(count_pattern 'Building CXX object' "$all_logs")"
    printf '  link steps: %s\n' "$(count_pattern 'Linking (C|CXX)' "$all_logs")"
    printf '  cargo fingerprint lines: %s\n' "$(count_pattern 'fingerprint' "$all_logs")"
    printf '\n'
    printf 'sccache before: %s\n' "$log_dir/sccache-before.txt"
    printf 'sccache after: %s\n' "$log_dir/sccache-after.txt"
} >> "$summary_file"

fingerprints_file="$log_dir/cargo-fingerprint-lines.txt"
grep -n 'fingerprint' "$all_logs" > "$fingerprints_file" 2>/dev/null || :

echo
echo "Summary: $summary_file"
sed -n '1,220p' "$summary_file"

exit "$failed"
