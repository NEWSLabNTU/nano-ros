#!/usr/bin/env bash
# Emit Zephyr fixture leaf records without building them.
#
# Prototype for Phase 226 fixture scheduling: keep just/zephyr-ci.just as the
# build owner for now, but centralize the matrix identity and derived settings.
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage: scripts/build/zephyr-fixture-leaves.sh --emit records [OPTIONS]

Emits one tab-separated Zephyr fixture leaf record per selected matrix row.
This diagnostic mode does not run west, ninja, cargo, cmake, or just.

Options:
  --emit records            required; emit fixture leaf records
  --zephyr-version VERSION  Zephyr line selector (default: $NROS_ZEPHYR_VERSION or 3.7)
  --nros-root DIR           nano-ros checkout root (default: current repo)
  --build-root DIR          Zephyr build root (default: $NROS_ZEPHYR_BUILD_ROOT,
                            else selected workspace path)
  --codegen-tool PATH       host codegen tool used in signatures
                            (default: resolved nros CLI, or build/host-codegen/nros-codegen
                            when nros is unavailable)
  --make-bin PATH           make path used in signatures (default: third-party/make/make
                            when executable, else command -v make)
  --toolchain-cache-dir DIR Zephyr toolchain capability cache dir
                            (default: build/zephyr-cache/ToolchainCapabilityDatabase)
  --sccache-disable 0|1     match NROS_ZEPHYR_SCCACHE_DISABLE (default: env or 0)
  --pristine auto|always|never
                            record desired pristine mode (default: env or auto)
  --filter REGEX            filter against "board build_dir src conf_files id"
                            (default: $NROS_ZEPHYR_FIXTURE_FILTER)
  --include-logging-smoke   also emit the Zephyr logging-smoke image leaf
  --include-workspace-entry also emit the Zephyr workspace-Entry leaf
  -h, --help                show this help

Record fields:
  kind id target board lang lang_tag role rmw src src_dir build_name build_dir
  log xrce_agent_port zenoh_locator cyclone_domain conf_files extra_cmake_defs
  sig sig_file best_effort eff_pristine
EOF
}

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
default_nros_root="$(cd "$script_dir/../.." && pwd)"

emit=""
zephyr_version="${NROS_ZEPHYR_VERSION:-3.7}"
nros_root="$default_nros_root"
build_root=""
codegen_tool=""
make_bin=""
toolchain_cache_dir=""
sccache_disable="${NROS_ZEPHYR_SCCACHE_DISABLE:-0}"
pristine="${NROS_ZEPHYR_PRISTINE:-auto}"
fixture_filter="${NROS_ZEPHYR_FIXTURE_FILTER:-}"
include_logging_smoke=0
include_workspace_entry=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --emit)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            emit="$1"
            ;;
        --zephyr-version)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            zephyr_version="$1"
            ;;
        --nros-root)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            nros_root="$1"
            ;;
        --build-root)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            build_root="$1"
            ;;
        --codegen-tool)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            codegen_tool="$1"
            ;;
        --make-bin)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            make_bin="$1"
            ;;
        --toolchain-cache-dir)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            toolchain_cache_dir="$1"
            ;;
        --sccache-disable)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            sccache_disable="$1"
            ;;
        --pristine)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            pristine="$1"
            ;;
        --filter)
            shift
            [ "$#" -gt 0 ] || { usage; exit 2; }
            fixture_filter="$1"
            ;;
        --include-logging-smoke)
            include_logging_smoke=1
            ;;
        --include-workspace-entry)
            include_workspace_entry=1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage
            exit 2
            ;;
    esac
    shift
done

if [ "$emit" != "records" ]; then
    usage
    exit 2
fi

case "$zephyr_version" in
    3.7|4.4) ;;
    *)
        echo "zephyr-fixture-leaves: unsupported --zephyr-version=$zephyr_version" >&2
        exit 2
        ;;
esac
case "$sccache_disable" in
    0|1) ;;
    *)
        echo "zephyr-fixture-leaves: invalid --sccache-disable=$sccache_disable; expected 0 or 1" >&2
        exit 2
        ;;
esac
case "$pristine" in
    auto|always|never) ;;
    *)
        echo "zephyr-fixture-leaves: invalid --pristine=$pristine; expected auto, always, or never" >&2
        exit 2
        ;;
esac

nros_root="$(cd "$nros_root" && pwd)"
# shellcheck source=scripts/build/fixture-matrix.sh
source "$nros_root/scripts/build/fixture-matrix.sh"
# shellcheck source=scripts/build/cargo.sh
source "$nros_root/scripts/build/cargo.sh"

if [ -z "$build_root" ]; then
    if [ -n "${NROS_ZEPHYR_BUILD_ROOT:-}" ]; then
        build_root="$NROS_ZEPHYR_BUILD_ROOT"
    elif [ "$zephyr_version" = "4.4" ]; then
        build_root="$nros_root/../nano-ros-workspace-4.4"
    elif [ -d "$nros_root/zephyr-workspace" ]; then
        build_root="$nros_root/zephyr-workspace"
    elif [ -d "$nros_root/../nano-ros-workspace" ]; then
        build_root="$nros_root/../nano-ros-workspace"
    else
        build_root="$nros_root/zephyr-workspace"
    fi
fi
if [ -z "$codegen_tool" ]; then
    codegen_tool="$(nros_cargo_codegen_c_bin 2>/dev/null || true)"
    if [ -z "$codegen_tool" ]; then
        codegen_tool="$nros_root/build/host-codegen/nros-codegen"
    fi
fi
if [ -z "$toolchain_cache_dir" ]; then
    toolchain_cache_dir="$nros_root/build/zephyr-cache/ToolchainCapabilityDatabase"
fi
if [ -z "$make_bin" ]; then
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
fi

log_dir="$nros_root/build/zephyr-fixtures"
build_root="$(realpath -m "$build_root")"
codegen_tool="$(realpath -m "$codegen_tool")"
toolchain_cache_dir="$(realpath -m "$toolchain_cache_dir")"
if [ -n "$make_bin" ]; then
    make_bin="$(realpath -m "$make_bin")"
fi

if [ "$zephyr_version" = "4.4" ]; then
    native_sim_nsos_conf="$nros_root/cmake/zephyr/native-sim-line-4.4.conf"
else
    native_sim_nsos_conf="$nros_root/zephyr/native-sim-nsos.conf"
fi

fixture_rmws=(zenoh)
if [ "$zephyr_version" != "4.4" ]; then
    fixture_rmws=(zenoh xrce)
    nros_store_root="${NROS_HOME:-$HOME/.nros}/sdk"
    if command -v idlc >/dev/null 2>&1 \
        || [ -n "$(ls "$nros_store_root"/cyclonedds/*/bin/idlc 2>/dev/null)" ] \
        || [ -x "$nros_root/build/cyclonedds/bin/idlc" ] \
        || [ -x "$nros_root/build/install/bin/idlc" ]; then
        fixture_rmws+=(cyclonedds)
    fi
fi

escape_field() {
    local value="$1"
    value="${value//$'\t'/\\t}"
    value="${value//$'\n'/\\n}"
    printf '%s' "$value"
}

emit_record() {
    local kind="$1"
    local id="$2"
    local target="$3"
    local board="$4"
    local lang="$5"
    local lang_tag="$6"
    local role="$7"
    local rmw="$8"
    local src="$9"
    local src_dir="${10}"
    local build_name="${11}"
    local build_dir="${12}"
    local log="${13}"
    local xrce_agent_port="${14}"
    local zenoh_locator="${15}"
    local cyclone_domain="${16}"
    local conf_files="${17}"
    local extra_cmake_defs="${18}"
    local sig="${19}"
    local sig_file="${20}"
    local best_effort="${21}"
    local eff_pristine="${22}"

    local fields=(
        "$kind" "$id" "$target" "$board" "$lang" "$lang_tag" "$role" "$rmw"
        "$src" "$src_dir" "$build_name" "$build_dir" "$log" "$xrce_agent_port"
        "$zenoh_locator" "$cyclone_domain" "$conf_files" "$extra_cmake_defs"
        "$sig" "$sig_file" "$best_effort" "$eff_pristine"
    )
    local i
    for i in "${!fields[@]}"; do
        if [ "$i" -gt 0 ]; then
            printf '\t'
        fi
        escape_field "${fields[$i]}"
    done
    printf '\n'
}

variant_offset_for_role() {
    case "$1" in
        talker|listener) printf '%s\n' 0 ;;
        service-server|service-client) printf '%s\n' 10 ;;
        action-server|action-client) printf '%s\n' 20 ;;
        *) echo "unknown Zephyr fixture role: $1" >&2; return 2 ;;
    esac
}

variant_idx_for_role() {
    case "$1" in
        talker|listener) printf '%s\n' 0 ;;
        service-server|service-client) printf '%s\n' 1 ;;
        action-server|action-client) printf '%s\n' 2 ;;
        *) echo "unknown Zephyr fixture role: $1" >&2; return 2 ;;
    esac
}

lang_offset_for_lang() {
    case "$1" in
        rust) printf '%s\n' 0 ;;
        c) printf '%s\n' 100 ;;
        cpp) printf '%s\n' 200 ;;
        *) echo "unknown Zephyr fixture language: $1" >&2; return 2 ;;
    esac
}

lang_idx_for_lang() {
    case "$1" in
        rust) printf '%s\n' 0 ;;
        c) printf '%s\n' 1 ;;
        cpp) printf '%s\n' 2 ;;
        *) echo "unknown Zephyr fixture language: $1" >&2; return 2 ;;
    esac
}

selected=0
for lang in $(nros_fixture_langs); do
    lang_tag="$(nros_zephyr_lang_tag "$lang")"
    lang_offset="$(lang_offset_for_lang "$lang")"
    lang_idx="$(lang_idx_for_lang "$lang")"
    for rmw in "${fixture_rmws[@]}"; do
        for role in $(nros_fixture_roles); do
            board="native_sim/native/64"
            build_name="build-${lang_tag}-${role}-${rmw}"
            build_dir="$build_root/$build_name"
            src="zephyr/${lang}/${role}"
            src_dir="$nros_root/examples/$src"
            best_effort=0
            xrce_agent_port=""
            zenoh_locator=""
            cyclone_domain=""
            variant_offset="$(variant_offset_for_role "$role")"
            variant_idx="$(variant_idx_for_role "$role")"
            conf_files="prj.conf;prj-${rmw}.conf;$native_sim_nsos_conf"
            extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"

            if [ "$rmw" = "xrce" ]; then
                xrce_agent_port=$((2018 + lang_offset + variant_offset))
                extra_cmake_defs="$extra_cmake_defs -DCONFIG_NROS_XRCE_AGENT_PORT=$xrce_agent_port"
            fi
            if [ "$rmw" = "zenoh" ]; then
                zenoh_port=$((7456 + lang_offset + variant_offset))
                zenoh_locator="tcp/127.0.0.1:$zenoh_port"
                extra_cmake_defs="$extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$zenoh_locator\""
            fi
            if [ "$rmw" = "cyclonedds" ]; then
                cyclone_domain=$((50 + lang_idx * 3 + variant_idx))
                extra_cmake_defs="$extra_cmake_defs -DCONFIG_NROS_DOMAIN_ID=$cyclone_domain"
            fi
            extra_cmake_defs="$extra_cmake_defs -DCONF_FILE=$conf_files"

            sccache_launcher=0
            if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
                sccache_launcher=1
                extra_cmake_defs="$extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
            fi

            id="zephyr/${board}/${lang}/${role}/${rmw}"
            target="fixture/zephyr/${board}/${lang}/${role}/${rmw}"
            filter_haystack="$board $build_name $src $conf_files $id"
            if [ -n "$fixture_filter" ] && ! [[ "$filter_haystack" =~ $fixture_filter ]]; then
                continue
            fi
            selected=$((selected + 1))
            log="$log_dir/${build_name}.log"
            sig_file="$build_dir/.nros-zephyr-fixture.sig"
            sig="$(printf '%s\n' \
                "board=$board" \
                "src=$src" \
                "xrce_port=$xrce_agent_port" \
                "conf_files=$conf_files" \
                "zenoh_locator=$zenoh_locator" \
                "codegen_tool=$codegen_tool" \
                "toolchain_cache_dir=$toolchain_cache_dir" \
                "make=$make_bin" \
                "sccache_launcher=$sccache_launcher")"
            emit_record fixture "$id" "$target" "$board" "$lang" "$lang_tag" "$role" "$rmw" \
                "$src" "$src_dir" "$build_name" "$build_dir" "$log" "$xrce_agent_port" \
                "$zenoh_locator" "$cyclone_domain" "$conf_files" "$extra_cmake_defs" \
                "$sig" "$sig_file" "$best_effort" "$pristine"
        done
    done
done

if [ "$include_logging_smoke" = "1" ]; then
    id="zephyr/native_sim/native/64/logging-smoke"
    target="fixture/zephyr/native_sim/native/64/logging-smoke"
    build_name="logging-smoke-zephyr-native-sim"
    build_dir="$build_root/$build_name"
    filter_haystack="native_sim/native/64 $build_name logging-smoke-zephyr-native-sim $id"
    if [ -z "$fixture_filter" ] || [[ "$filter_haystack" =~ $fixture_filter ]]; then
        selected=$((selected + 1))
        emit_record fixture "$id" "$target" "native_sim/native/64" rust rs logging-smoke default \
            "packages/testing/nros-tests/bins/logging-smoke-zephyr-native-sim" \
            "$nros_root/packages/testing/nros-tests/bins/logging-smoke-zephyr-native-sim" \
            "$build_name" "$build_dir" "$log_dir/${build_name}.log" "" "" "" "" "" "" \
            "$build_dir/.nros-zephyr-fixture.sig" 0 "$pristine"
    fi
fi

# Phase 225.P.6 — workspace-Entry leaf (Approach A). Constructed directly,
# bypassing variant_offset_for_role (role="entry" is unknown to it). The
# proven zephyr-fixture-run-one.sh west path builds it unchanged from the
# Zephyr application dir at examples/workspaces/rust/src/zephyr_entry. The
# Entry is a rust+pubsub workload, so it bakes the same Zephyr rust-pubsub
# locator (port 7456 = 7456 + lang_offset 0 + variant_offset 0) the E2E
# test's zenohd router listens on. It therefore shares that port with the
# single-node rust pubsub talker and MUST serialize with it — the E2E test
# is routed into the `qemu-zephyr-pubsub-rust` nextest group.
if [ "$include_workspace_entry" = "1" ]; then
    ws_board="native_sim/native/64"
    ws_lang="rust"
    ws_lang_tag="rs"
    ws_role="entry"
    ws_rmw="zenoh"
    ws_build_name="build-ws-rs-entry-zenoh"
    ws_build_dir="$build_root/$ws_build_name"
    ws_src="workspaces/rust/src/zephyr_entry"
    ws_src_dir="$nros_root/examples/$ws_src"
    ws_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    ws_zenoh_locator="tcp/127.0.0.1:7456"
    ws_id="zephyr/native_sim/native/64/workspace-entry"
    ws_target="fixture/zephyr/native_sim/native/64/workspace-entry"
    ws_filter_haystack="$ws_board $ws_build_name $ws_src $ws_conf_files $ws_id"
    if [ -z "$fixture_filter" ] || [[ "$ws_filter_haystack" =~ $fixture_filter ]]; then
        selected=$((selected + 1))
        ws_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        ws_extra_cmake_defs="$ws_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$ws_zenoh_locator\""
        ws_extra_cmake_defs="$ws_extra_cmake_defs -DCONF_FILE=$ws_conf_files"
        ws_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            ws_sccache_launcher=1
            ws_extra_cmake_defs="$ws_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        ws_sig_file="$ws_build_dir/.nros-zephyr-fixture.sig"
        ws_sig="$(printf '%s\n' \
            "board=$ws_board" \
            "src=$ws_src" \
            "xrce_port=" \
            "conf_files=$ws_conf_files" \
            "zenoh_locator=$ws_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$ws_sccache_launcher")"
        emit_record fixture "$ws_id" "$ws_target" "$ws_board" "$ws_lang" "$ws_lang_tag" "$ws_role" "$ws_rmw" \
            "$ws_src" "$ws_src_dir" "$ws_build_name" "$ws_build_dir" "$log_dir/${ws_build_name}.log" "" \
            "$ws_zenoh_locator" "" "$ws_conf_files" "$ws_extra_cmake_defs" \
            "$ws_sig" "$ws_sig_file" 0 "$pristine"
    fi
fi

if [ "$selected" -eq 0 ]; then
    echo "zephyr-fixture-leaves: no records matched filter: $fixture_filter" >&2
    exit 1
fi
