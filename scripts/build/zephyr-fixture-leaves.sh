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

    # phase-276 W1 (#128) — the PARAMETERISED Rust workspace Entry
    # (examples/workspaces/ws-params-rust/src/zephyr_entry): the params-on-embedded
    # coverage cell. Same native_sim/NSOS west path as the base Rust workspace
    # entry above; `system.toml [param_services]` + the entry's
    # `nros/param-services` feature make the (#128-fixed) Framework::Zephyr emit
    # register the six ROS 2 parameter services. Dials a DISTINCT zenohd port
    # (17845), so it does NOT serialize with the pubsub fixtures. Consumed by
    # tests/entry_e2e.rs (zephyr_rust_params cell).
    wsp_board="native_sim/native/64"
    wsp_lang="rust"
    wsp_lang_tag="rs"
    wsp_role="entry"
    wsp_rmw="zenoh"
    wsp_build_name="build-ws-rs-params-entry-zenoh"
    wsp_build_dir="$build_root/$wsp_build_name"
    wsp_src="workspaces/ws-params-rust/src/zephyr_entry"
    wsp_src_dir="$nros_root/examples/$wsp_src"
    wsp_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wsp_zenoh_locator="tcp/127.0.0.1:17845"
    wsp_id="zephyr/native_sim/native/64/workspace-params-entry"
    wsp_target="fixture/zephyr/native_sim/native/64/workspace-params-entry"
    wsp_filter_haystack="$wsp_board $wsp_build_name $wsp_src $wsp_conf_files $wsp_id"
    if [ -d "$wsp_src_dir" ] && { [ -z "$fixture_filter" ] || [[ "$wsp_filter_haystack" =~ $fixture_filter ]]; }; then
        selected=$((selected + 1))
        wsp_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wsp_extra_cmake_defs="$wsp_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wsp_zenoh_locator\""
        wsp_extra_cmake_defs="$wsp_extra_cmake_defs -DCONF_FILE=$wsp_conf_files"
        wsp_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wsp_sccache_launcher=1
            wsp_extra_cmake_defs="$wsp_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wsp_sig_file="$wsp_build_dir/.nros-zephyr-fixture.sig"
        wsp_sig="$(printf '%s\n' \
            "board=$wsp_board" \
            "src=$wsp_src" \
            "xrce_port=" \
            "conf_files=$wsp_conf_files" \
            "zenoh_locator=$wsp_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wsp_sccache_launcher")"
        emit_record fixture "$wsp_id" "$wsp_target" "$wsp_board" "$wsp_lang" "$wsp_lang_tag" "$wsp_role" "$wsp_rmw" \
            "$wsp_src" "$wsp_src_dir" "$wsp_build_name" "$wsp_build_dir" "$log_dir/${wsp_build_name}.log" "" \
            "$wsp_zenoh_locator" "" "$wsp_conf_files" "$wsp_extra_cmake_defs" \
            "$wsp_sig" "$wsp_sig_file" 0 "$pristine"
    fi

    # phase-276 W3 (#128) — the MANAGED (lifecycle) Rust workspace Entry
    # (examples/workspaces/ws-lifecycle-rust/src/zephyr_entry): the
    # lifecycle-on-embedded coverage cell. `system.toml [lifecycle]
    # autostart = "active"` + the entry's `nros/lifecycle-services` feature make
    # the (#128-fixed) Framework::Zephyr emit register the five REP-2002
    # lifecycle services + drive the boot autostart. Distinct zenohd port
    # (17847). Consumed by tests/entry_e2e.rs (zephyr_rust_lifecycle cell).
    wsl_board="native_sim/native/64"
    wsl_lang="rust"
    wsl_lang_tag="rs"
    wsl_role="entry"
    wsl_rmw="zenoh"
    wsl_build_name="build-ws-rs-lifecycle-entry-zenoh"
    wsl_build_dir="$build_root/$wsl_build_name"
    wsl_src="workspaces/ws-lifecycle-rust/src/zephyr_entry"
    wsl_src_dir="$nros_root/examples/$wsl_src"
    wsl_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wsl_zenoh_locator="tcp/127.0.0.1:17847"
    wsl_id="zephyr/native_sim/native/64/workspace-lifecycle-entry"
    wsl_target="fixture/zephyr/native_sim/native/64/workspace-lifecycle-entry"
    wsl_filter_haystack="$wsl_board $wsl_build_name $wsl_src $wsl_conf_files $wsl_id"
    if [ -d "$wsl_src_dir" ] && { [ -z "$fixture_filter" ] || [[ "$wsl_filter_haystack" =~ $fixture_filter ]]; }; then
        selected=$((selected + 1))
        wsl_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wsl_extra_cmake_defs="$wsl_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wsl_zenoh_locator\""
        wsl_extra_cmake_defs="$wsl_extra_cmake_defs -DCONF_FILE=$wsl_conf_files"
        wsl_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wsl_sccache_launcher=1
            wsl_extra_cmake_defs="$wsl_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wsl_sig_file="$wsl_build_dir/.nros-zephyr-fixture.sig"
        wsl_sig="$(printf '%s\n' \
            "board=$wsl_board" \
            "src=$wsl_src" \
            "xrce_port=" \
            "conf_files=$wsl_conf_files" \
            "zenoh_locator=$wsl_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wsl_sccache_launcher")"
        emit_record fixture "$wsl_id" "$wsl_target" "$wsl_board" "$wsl_lang" "$wsl_lang_tag" "$wsl_role" "$wsl_rmw" \
            "$wsl_src" "$wsl_src_dir" "$wsl_build_name" "$wsl_build_dir" "$log_dir/${wsl_build_name}.log" "" \
            "$wsl_zenoh_locator" "" "$wsl_conf_files" "$wsl_extra_cmake_defs" \
            "$wsl_sig" "$wsl_sig_file" 0 "$pristine"
    fi

    # phase-276 W5 — the QOS-OVERRIDE Rust workspace Entry
    # (examples/workspaces/ws-qos-rust/src/zephyr_entry): the qos-on-embedded
    # coverage cell. The QoS profiles are declared per-entity in node code
    # (RFC-0041, reliable + transient_local on /qos_chatter); the on-target
    # QoS-matched pair republishes its receive count on /qos_ok. Distinct zenohd
    # port (17849). Consumed by tests/entry_e2e.rs (zephyr_rust_qos cell).
    wsq_board="native_sim/native/64"
    wsq_lang="rust"
    wsq_lang_tag="rs"
    wsq_role="entry"
    wsq_rmw="zenoh"
    wsq_build_name="build-ws-rs-qos-entry-zenoh"
    wsq_build_dir="$build_root/$wsq_build_name"
    wsq_src="workspaces/ws-qos-rust/src/zephyr_entry"
    wsq_src_dir="$nros_root/examples/$wsq_src"
    wsq_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wsq_zenoh_locator="tcp/127.0.0.1:17849"
    wsq_id="zephyr/native_sim/native/64/workspace-qos-entry"
    wsq_target="fixture/zephyr/native_sim/native/64/workspace-qos-entry"
    wsq_filter_haystack="$wsq_board $wsq_build_name $wsq_src $wsq_conf_files $wsq_id"
    if [ -d "$wsq_src_dir" ] && { [ -z "$fixture_filter" ] || [[ "$wsq_filter_haystack" =~ $fixture_filter ]]; }; then
        selected=$((selected + 1))
        wsq_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wsq_extra_cmake_defs="$wsq_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wsq_zenoh_locator\""
        wsq_extra_cmake_defs="$wsq_extra_cmake_defs -DCONF_FILE=$wsq_conf_files"
        wsq_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wsq_sccache_launcher=1
            wsq_extra_cmake_defs="$wsq_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wsq_sig_file="$wsq_build_dir/.nros-zephyr-fixture.sig"
        wsq_sig="$(printf '%s\n' \
            "board=$wsq_board" \
            "src=$wsq_src" \
            "xrce_port=" \
            "conf_files=$wsq_conf_files" \
            "zenoh_locator=$wsq_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wsq_sccache_launcher")"
        emit_record fixture "$wsq_id" "$wsq_target" "$wsq_board" "$wsq_lang" "$wsq_lang_tag" "$wsq_role" "$wsq_rmw" \
            "$wsq_src" "$wsq_src_dir" "$wsq_build_name" "$wsq_build_dir" "$log_dir/${wsq_build_name}.log" "" \
            "$wsq_zenoh_locator" "" "$wsq_conf_files" "$wsq_extra_cmake_defs" \
            "$wsq_sig" "$wsq_sig_file" 0 "$pristine"
    fi

    # phase-276 W4 — the E2E-SAFETY (CRC) Rust workspace Entry
    # (examples/workspaces/ws-safety-rust/src/zephyr_entry): the
    # safety-on-embedded coverage cell. The system declares
    # [system].features = ["safety"] → the zenoh backend attaches the E2E CRC +
    # sequence number on publish and validates on receive; the on-target
    # safe_listener republishes its CRC-VALIDATED count on /safe_ok. Distinct
    # zenohd port (17851). Consumed by tests/entry_e2e.rs (zephyr_rust_safety cell).
    wss_board="native_sim/native/64"
    wss_lang="rust"
    wss_lang_tag="rs"
    wss_role="entry"
    wss_rmw="zenoh"
    wss_build_name="build-ws-rs-safety-entry-zenoh"
    wss_build_dir="$build_root/$wss_build_name"
    wss_src="workspaces/ws-safety-rust/src/zephyr_entry"
    wss_src_dir="$nros_root/examples/$wss_src"
    wss_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wss_zenoh_locator="tcp/127.0.0.1:17851"
    wss_id="zephyr/native_sim/native/64/workspace-safety-entry"
    wss_target="fixture/zephyr/native_sim/native/64/workspace-safety-entry"
    wss_filter_haystack="$wss_board $wss_build_name $wss_src $wss_conf_files $wss_id"
    if [ -d "$wss_src_dir" ] && { [ -z "$fixture_filter" ] || [[ "$wss_filter_haystack" =~ $fixture_filter ]]; }; then
        selected=$((selected + 1))
        wss_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wss_extra_cmake_defs="$wss_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wss_zenoh_locator\""
        wss_extra_cmake_defs="$wss_extra_cmake_defs -DCONF_FILE=$wss_conf_files"
        wss_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wss_sccache_launcher=1
            wss_extra_cmake_defs="$wss_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wss_sig_file="$wss_build_dir/.nros-zephyr-fixture.sig"
        wss_sig="$(printf '%s\n' \
            "board=$wss_board" \
            "src=$wss_src" \
            "xrce_port=" \
            "conf_files=$wss_conf_files" \
            "zenoh_locator=$wss_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wss_sccache_launcher")"
        emit_record fixture "$wss_id" "$wss_target" "$wss_board" "$wss_lang" "$wss_lang_tag" "$wss_role" "$wss_rmw" \
            "$wss_src" "$wss_src_dir" "$wss_build_name" "$wss_build_dir" "$log_dir/${wss_build_name}.log" "" \
            "$wss_zenoh_locator" "" "$wss_conf_files" "$wss_extra_cmake_defs" \
            "$wss_sig" "$wss_sig_file" 0 "$pristine"
    fi

    # phase-276 W6 — the MULTIHOST robot1 (talker) Rust workspace Entry
    # (examples/workspaces/rust/src/zephyr_entry_robot1): the
    # multihost-on-embedded coverage cell. `nros::main!(launch =
    # "demo_bringup:multihost.launch.xml", host = "robot1")` bakes only the
    # robot1 slice (the talker); the robot2 listener is a native per-host
    # entry in the paired e2e, so /chatter crosses hosts. Distinct zenohd
    # port (17853). Consumed by tests/multihost_e2e.rs (zephyr_rust cell).
    wsm_board="native_sim/native/64"
    wsm_lang="rust"
    wsm_lang_tag="rs"
    wsm_role="entry"
    wsm_rmw="zenoh"
    wsm_build_name="build-ws-rs-mh-robot1-entry-zenoh"
    wsm_build_dir="$build_root/$wsm_build_name"
    wsm_src="workspaces/rust/src/zephyr_entry_robot1"
    wsm_src_dir="$nros_root/examples/$wsm_src"
    wsm_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wsm_zenoh_locator="tcp/127.0.0.1:17853"
    wsm_id="zephyr/native_sim/native/64/workspace-mh-robot1-entry"
    wsm_target="fixture/zephyr/native_sim/native/64/workspace-mh-robot1-entry"
    wsm_filter_haystack="$wsm_board $wsm_build_name $wsm_src $wsm_conf_files $wsm_id"
    if [ -d "$wsm_src_dir" ] && { [ -z "$fixture_filter" ] || [[ "$wsm_filter_haystack" =~ $fixture_filter ]]; }; then
        selected=$((selected + 1))
        wsm_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wsm_extra_cmake_defs="$wsm_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wsm_zenoh_locator\""
        wsm_extra_cmake_defs="$wsm_extra_cmake_defs -DCONF_FILE=$wsm_conf_files"
        wsm_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wsm_sccache_launcher=1
            wsm_extra_cmake_defs="$wsm_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wsm_sig_file="$wsm_build_dir/.nros-zephyr-fixture.sig"
        wsm_sig="$(printf '%s\n' \
            "board=$wsm_board" \
            "src=$wsm_src" \
            "xrce_port=" \
            "conf_files=$wsm_conf_files" \
            "zenoh_locator=$wsm_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wsm_sccache_launcher")"
        emit_record fixture "$wsm_id" "$wsm_target" "$wsm_board" "$wsm_lang" "$wsm_lang_tag" "$wsm_role" "$wsm_rmw" \
            "$wsm_src" "$wsm_src_dir" "$wsm_build_name" "$wsm_build_dir" "$log_dir/${wsm_build_name}.log" "" \
            "$wsm_zenoh_locator" "" "$wsm_conf_files" "$wsm_extra_cmake_defs" \
            "$wsm_sig" "$wsm_sig_file" 0 "$pristine"
    fi

    # phase-276 W2 / issue #128 half 2 — the RT-TIERS Rust workspace Entry
    # (examples/workspaces/ws-realtime-rust/src/zephyr_entry): the
    # tiers-on-embedded coverage cell. system.toml declares two [tiers.*]
    # with [tiers.*.zephyr] priorities, so the macro emits
    # ZephyrBoard::run_tiers — one k_thread per tier over ONE shared session
    # (RFC-0015 Model 1); ctrl (10 ms) + telem (100 ms) publish /ctrl +
    # /telem. Distinct zenohd port (17855). Consumed by
    # tests/realtime_tiers_e2e.rs (zephyr_rust cell).
    wst_board="native_sim/native/64"
    wst_lang="rust"
    wst_lang_tag="rs"
    wst_role="entry"
    wst_rmw="zenoh"
    wst_build_name="build-ws-rs-realtime-entry-zenoh"
    wst_build_dir="$build_root/$wst_build_name"
    wst_src="workspaces/ws-realtime-rust/src/zephyr_entry"
    wst_src_dir="$nros_root/examples/$wst_src"
    wst_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wst_zenoh_locator="tcp/127.0.0.1:17855"
    wst_id="zephyr/native_sim/native/64/workspace-realtime-entry"
    wst_target="fixture/zephyr/native_sim/native/64/workspace-realtime-entry"
    wst_filter_haystack="$wst_board $wst_build_name $wst_src $wst_conf_files $wst_id"
    if [ -d "$wst_src_dir" ] && { [ -z "$fixture_filter" ] || [[ "$wst_filter_haystack" =~ $fixture_filter ]]; }; then
        selected=$((selected + 1))
        wst_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wst_extra_cmake_defs="$wst_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wst_zenoh_locator\""
        wst_extra_cmake_defs="$wst_extra_cmake_defs -DCONF_FILE=$wst_conf_files"
        wst_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wst_sccache_launcher=1
            wst_extra_cmake_defs="$wst_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wst_sig_file="$wst_build_dir/.nros-zephyr-fixture.sig"
        wst_sig="$(printf '%s\n' \
            "board=$wst_board" \
            "src=$wst_src" \
            "xrce_port=" \
            "conf_files=$wst_conf_files" \
            "zenoh_locator=$wst_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wst_sccache_launcher")"
        emit_record fixture "$wst_id" "$wst_target" "$wst_board" "$wst_lang" "$wst_lang_tag" "$wst_role" "$wst_rmw" \
            "$wst_src" "$wst_src_dir" "$wst_build_name" "$wst_build_dir" "$log_dir/${wst_build_name}.log" "" \
            "$wst_zenoh_locator" "" "$wst_conf_files" "$wst_extra_cmake_defs" \
            "$wst_sig" "$wst_sig_file" 0 "$pristine"
    fi

    # phase-281 W3b — the RT-TIERS C++ workspace Entry
    # (examples/workspaces/ws-realtime-cpp/src/zephyr_entry): the FIRST full west
    # link + runtime proof of the W3a ZephyrBoard::run_tiers seam. The C++ sibling
    # of the rust realtime entry above; demo_bringup/system.toml declares two
    # [tiers.*] with [tiers.*.zephyr] priorities, so the C++ codegen emits a plain
    # int main(void) calling ZephyrBoard::run_tiers (nros_board_zephyr_run_tiers) —
    # one k_thread per tier over ONE shared session (RFC-0015 Model 1); ctrl (10 ms,
    # high) publishes /ctrl, telem (100 ms, low) publishes /telem. CONFIG_NROS_CPP_API
    # (prj.conf) compiles the W3a seam. Distinct zenohd port (17857). Consumed by
    # tests/realtime_tiers_e2e.rs (zephyr_cpp cell).
    wscpprt_board="native_sim/native/64"
    wscpprt_lang="cpp"
    wscpprt_lang_tag="cpp"
    wscpprt_role="entry"
    wscpprt_rmw="zenoh"
    wscpprt_build_name="build-ws-cpp-realtime-entry-zenoh"
    wscpprt_build_dir="$build_root/$wscpprt_build_name"
    wscpprt_src="workspaces/ws-realtime-cpp/src/zephyr_entry"
    wscpprt_src_dir="$nros_root/examples/$wscpprt_src"
    wscpprt_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wscpprt_zenoh_locator="tcp/127.0.0.1:17857"
    wscpprt_id="zephyr/native_sim/native/64/workspace-realtime-entry-cpp"
    wscpprt_target="fixture/zephyr/native_sim/native/64/workspace-realtime-entry-cpp"
    wscpprt_filter_haystack="$wscpprt_board $wscpprt_build_name $wscpprt_src $wscpprt_conf_files $wscpprt_id"
    if [ -d "$wscpprt_src_dir" ] && { [ -z "$fixture_filter" ] || [[ "$wscpprt_filter_haystack" =~ $fixture_filter ]]; }; then
        selected=$((selected + 1))
        wscpprt_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wscpprt_extra_cmake_defs="$wscpprt_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wscpprt_zenoh_locator\""
        wscpprt_extra_cmake_defs="$wscpprt_extra_cmake_defs -DCONF_FILE=$wscpprt_conf_files"
        wscpprt_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wscpprt_sccache_launcher=1
            wscpprt_extra_cmake_defs="$wscpprt_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wscpprt_sig_file="$wscpprt_build_dir/.nros-zephyr-fixture.sig"
        wscpprt_sig="$(printf '%s\n' \
            "board=$wscpprt_board" \
            "src=$wscpprt_src" \
            "xrce_port=" \
            "conf_files=$wscpprt_conf_files" \
            "zenoh_locator=$wscpprt_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wscpprt_sccache_launcher")"
        emit_record fixture "$wscpprt_id" "$wscpprt_target" "$wscpprt_board" "$wscpprt_lang" "$wscpprt_lang_tag" "$wscpprt_role" "$wscpprt_rmw" \
            "$wscpprt_src" "$wscpprt_src_dir" "$wscpprt_build_name" "$wscpprt_build_dir" "$log_dir/${wscpprt_build_name}.log" "" \
            "$wscpprt_zenoh_locator" "" "$wscpprt_conf_files" "$wscpprt_extra_cmake_defs" \
            "$wscpprt_sig" "$wscpprt_sig_file" 0 "$pristine"
    fi

    # phase-281 W3c — the RT-TIERS C workspace Entry
    # (examples/workspaces/ws-realtime-c/src/zephyr_entry): the FIRST full west link +
    # runtime proof of the W3a ZephyrBoard::run_tiers seam for a C node (the C sibling of the
    # wscpprt cpp realtime entry above; closes the c×zephyr cell). demo_bringup/system.toml
    # declares two [tiers.*] with [tiers.*.zephyr] priorities, so the C codegen emits a plain
    # int main(void) calling ZephyrBoard::run_tiers (nros_board_zephyr_run_tiers) — one
    # k_thread per tier over ONE shared session (RFC-0015 Model 1); ctrl (10 ms, high)
    # publishes /ctrl, telem (100 ms, low) publishes /telem. CONFIG_NROS_C_API (prj.conf)
    # compiles the W3a seam. Distinct zenohd port (17859). Consumed by
    # tests/realtime_tiers_e2e.rs (zephyr_c cell).
    wscrt_board="native_sim/native/64"
    wscrt_lang="c"
    wscrt_lang_tag="c"
    wscrt_role="entry"
    wscrt_rmw="zenoh"
    wscrt_build_name="build-ws-c-realtime-entry-zenoh"
    wscrt_build_dir="$build_root/$wscrt_build_name"
    wscrt_src="workspaces/ws-realtime-c/src/zephyr_entry"
    wscrt_src_dir="$nros_root/examples/$wscrt_src"
    wscrt_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wscrt_zenoh_locator="tcp/127.0.0.1:17859"
    wscrt_id="zephyr/native_sim/native/64/workspace-realtime-entry-c"
    wscrt_target="fixture/zephyr/native_sim/native/64/workspace-realtime-entry-c"
    wscrt_filter_haystack="$wscrt_board $wscrt_build_name $wscrt_src $wscrt_conf_files $wscrt_id"
    if [ -d "$wscrt_src_dir" ] && { [ -z "$fixture_filter" ] || [[ "$wscrt_filter_haystack" =~ $fixture_filter ]]; }; then
        selected=$((selected + 1))
        wscrt_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wscrt_extra_cmake_defs="$wscrt_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wscrt_zenoh_locator\""
        wscrt_extra_cmake_defs="$wscrt_extra_cmake_defs -DCONF_FILE=$wscrt_conf_files"
        wscrt_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wscrt_sccache_launcher=1
            wscrt_extra_cmake_defs="$wscrt_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wscrt_sig_file="$wscrt_build_dir/.nros-zephyr-fixture.sig"
        wscrt_sig="$(printf '%s\n' \
            "board=$wscrt_board" \
            "src=$wscrt_src" \
            "xrce_port=" \
            "conf_files=$wscrt_conf_files" \
            "zenoh_locator=$wscrt_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wscrt_sccache_launcher")"
        emit_record fixture "$wscrt_id" "$wscrt_target" "$wscrt_board" "$wscrt_lang" "$wscrt_lang_tag" "$wscrt_role" "$wscrt_rmw" \
            "$wscrt_src" "$wscrt_src_dir" "$wscrt_build_name" "$wscrt_build_dir" "$log_dir/${wscrt_build_name}.log" "" \
            "$wscrt_zenoh_locator" "" "$wscrt_conf_files" "$wscrt_extra_cmake_defs" \
            "$wscrt_sig" "$wscrt_sig_file" 0 "$pristine"
    fi

    # phase-263 C2d — the C WORKSPACE entry (Approach A). Same native_sim/NSOS west path as
    # the Rust workspace entry above, but the Zephyr application dir is
    # examples/workspaces/c/src/zephyr_entry (find_package(Zephyr) + nano_ros_entry(BOARD
    # zephyr …) — the C/C++ analog of rust_cargo_application()). It dials a DISTINCT zenohd
    # port (17831), so it does NOT serialize with the rust workspace/single-node pubsub
    # fixtures. Consumed by tests/entry_e2e.rs (zephyr_c cell).
    wsc_board="native_sim/native/64"
    wsc_lang="c"
    wsc_lang_tag="c"
    wsc_role="entry"
    wsc_rmw="zenoh"
    wsc_build_name="build-ws-c-entry-zenoh"
    wsc_build_dir="$build_root/$wsc_build_name"
    wsc_src="workspaces/c/src/zephyr_entry"
    wsc_src_dir="$nros_root/examples/$wsc_src"
    wsc_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wsc_zenoh_locator="tcp/127.0.0.1:17831"
    wsc_id="zephyr/native_sim/native/64/workspace-entry-c"
    wsc_target="fixture/zephyr/native_sim/native/64/workspace-entry-c"
    wsc_filter_haystack="$wsc_board $wsc_build_name $wsc_src $wsc_conf_files $wsc_id"
    if [ -z "$fixture_filter" ] || [[ "$wsc_filter_haystack" =~ $fixture_filter ]]; then
        selected=$((selected + 1))
        wsc_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wsc_extra_cmake_defs="$wsc_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wsc_zenoh_locator\""
        wsc_extra_cmake_defs="$wsc_extra_cmake_defs -DCONF_FILE=$wsc_conf_files"
        wsc_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wsc_sccache_launcher=1
            wsc_extra_cmake_defs="$wsc_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wsc_sig_file="$wsc_build_dir/.nros-zephyr-fixture.sig"
        wsc_sig="$(printf '%s\n' \
            "board=$wsc_board" \
            "src=$wsc_src" \
            "xrce_port=" \
            "conf_files=$wsc_conf_files" \
            "zenoh_locator=$wsc_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wsc_sccache_launcher")"
        emit_record fixture "$wsc_id" "$wsc_target" "$wsc_board" "$wsc_lang" "$wsc_lang_tag" "$wsc_role" "$wsc_rmw" \
            "$wsc_src" "$wsc_src_dir" "$wsc_build_name" "$wsc_build_dir" "$log_dir/${wsc_build_name}.log" "" \
            "$wsc_zenoh_locator" "" "$wsc_conf_files" "$wsc_extra_cmake_defs" \
            "$wsc_sig" "$wsc_sig_file" 0 "$pristine"
    fi

    # phase-263 C2c — the C++ WORKSPACE entry (typed std_msgs). Same native_sim/NSOS west
    # path as the C workspace entry; the Zephyr application dir is
    # examples/workspaces/cpp/src/zephyr_entry. Distinct zenohd port (17833). Consumed by
    # tests/entry_e2e.rs (zephyr_cpp cell).
    wscpp_board="native_sim/native/64"
    wscpp_lang="cpp"
    wscpp_lang_tag="cpp"
    wscpp_role="entry"
    wscpp_rmw="zenoh"
    wscpp_build_name="build-ws-cpp-entry-zenoh"
    wscpp_build_dir="$build_root/$wscpp_build_name"
    wscpp_src="workspaces/cpp/src/zephyr_entry"
    wscpp_src_dir="$nros_root/examples/$wscpp_src"
    wscpp_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wscpp_zenoh_locator="tcp/127.0.0.1:17833"
    wscpp_id="zephyr/native_sim/native/64/workspace-entry-cpp"
    wscpp_target="fixture/zephyr/native_sim/native/64/workspace-entry-cpp"
    wscpp_filter_haystack="$wscpp_board $wscpp_build_name $wscpp_src $wscpp_conf_files $wscpp_id"
    if [ -z "$fixture_filter" ] || [[ "$wscpp_filter_haystack" =~ $fixture_filter ]]; then
        selected=$((selected + 1))
        wscpp_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wscpp_extra_cmake_defs="$wscpp_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wscpp_zenoh_locator\""
        wscpp_extra_cmake_defs="$wscpp_extra_cmake_defs -DCONF_FILE=$wscpp_conf_files"
        wscpp_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wscpp_sccache_launcher=1
            wscpp_extra_cmake_defs="$wscpp_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wscpp_sig_file="$wscpp_build_dir/.nros-zephyr-fixture.sig"
        wscpp_sig="$(printf '%s\n' \
            "board=$wscpp_board" \
            "src=$wscpp_src" \
            "xrce_port=" \
            "conf_files=$wscpp_conf_files" \
            "zenoh_locator=$wscpp_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wscpp_sccache_launcher")"
        emit_record fixture "$wscpp_id" "$wscpp_target" "$wscpp_board" "$wscpp_lang" "$wscpp_lang_tag" "$wscpp_role" "$wscpp_rmw" \
            "$wscpp_src" "$wscpp_src_dir" "$wscpp_build_name" "$wscpp_build_dir" "$log_dir/${wscpp_build_name}.log" "" \
            "$wscpp_zenoh_locator" "" "$wscpp_conf_files" "$wscpp_extra_cmake_defs" \
            "$wscpp_sig" "$wscpp_sig_file" 0 "$pristine"
    fi

    # phase-263 C2c-zephyr — the MIXED WORKSPACE entry (C talker + C++ listener + Rust
    # heartbeat). Same native_sim/NSOS west path as the cpp entry; the Zephyr application dir
    # is examples/workspaces/mixed/src/zephyr_entry. The entry sets NROS_WS_RUST_NODE_DIRS so
    # the nano-ros module bundles the Rust node into the nros_ws_runtime umbrella staticlib
    # (single Rust runtime). Distinct zenohd port (17843). Consumed by
    # tests/entry_e2e.rs (zephyr_mixed cell).
    wsmx_board="native_sim/native/64"
    wsmx_lang="mixed"
    wsmx_lang_tag="mixed"
    wsmx_role="entry"
    wsmx_rmw="zenoh"
    wsmx_build_name="build-ws-mixed-entry-zenoh"
    wsmx_build_dir="$build_root/$wsmx_build_name"
    wsmx_src="workspaces/mixed/src/zephyr_entry"
    wsmx_src_dir="$nros_root/examples/$wsmx_src"
    wsmx_conf_files="prj.conf;prj-zenoh.conf;$native_sim_nsos_conf"
    wsmx_zenoh_locator="tcp/127.0.0.1:17843"
    wsmx_id="zephyr/native_sim/native/64/workspace-entry-mixed"
    wsmx_target="fixture/zephyr/native_sim/native/64/workspace-entry-mixed"
    wsmx_filter_haystack="$wsmx_board $wsmx_build_name $wsmx_src $wsmx_conf_files $wsmx_id"
    if [ -z "$fixture_filter" ] || [[ "$wsmx_filter_haystack" =~ $fixture_filter ]]; then
        selected=$((selected + 1))
        wsmx_extra_cmake_defs="-D_NANO_ROS_CODEGEN_TOOL=$codegen_tool -DZEPHYR_TOOLCHAIN_CAPABILITY_CACHE_DIR=$toolchain_cache_dir -DMAKE=$make_bin -DUSE_CCACHE=0"
        wsmx_extra_cmake_defs="$wsmx_extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$wsmx_zenoh_locator\""
        wsmx_extra_cmake_defs="$wsmx_extra_cmake_defs -DCONF_FILE=$wsmx_conf_files"
        wsmx_sccache_launcher=0
        if [ "$sccache_disable" = "0" ] && command -v sccache >/dev/null 2>&1; then
            wsmx_sccache_launcher=1
            wsmx_extra_cmake_defs="$wsmx_extra_cmake_defs -DCMAKE_C_COMPILER_LAUNCHER=sccache -DCMAKE_CXX_COMPILER_LAUNCHER=sccache"
        fi
        wsmx_sig_file="$wsmx_build_dir/.nros-zephyr-fixture.sig"
        wsmx_sig="$(printf '%s\n' \
            "board=$wsmx_board" \
            "src=$wsmx_src" \
            "xrce_port=" \
            "conf_files=$wsmx_conf_files" \
            "zenoh_locator=$wsmx_zenoh_locator" \
            "codegen_tool=$codegen_tool" \
            "toolchain_cache_dir=$toolchain_cache_dir" \
            "make=$make_bin" \
            "sccache_launcher=$wsmx_sccache_launcher")"
        emit_record fixture "$wsmx_id" "$wsmx_target" "$wsmx_board" "$wsmx_lang" "$wsmx_lang_tag" "$wsmx_role" "$wsmx_rmw" \
            "$wsmx_src" "$wsmx_src_dir" "$wsmx_build_name" "$wsmx_build_dir" "$log_dir/${wsmx_build_name}.log" "" \
            "$wsmx_zenoh_locator" "" "$wsmx_conf_files" "$wsmx_extra_cmake_defs" \
            "$wsmx_sig" "$wsmx_sig_file" 0 "$pristine"
    fi
fi

if [ "$selected" -eq 0 ]; then
    echo "zephyr-fixture-leaves: no records matched filter: $fixture_filter" >&2
    exit 1
fi
