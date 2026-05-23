#!/usr/bin/env bash

# Run `nros generate-rust --force` only when package/codegen/interface inputs
# changed. The generator itself writes files idempotently, but avoiding the
# process entirely saves repeated ament-index scans across the full example tree.

nros_generate_rust_output_dir() {
    local output="generated"
    while [ "$#" -gt 0 ]; do
        case "$1" in
            -o|--output)
                if [ "$#" -gt 1 ]; then
                    output="$2"
                    shift
                fi
                ;;
            --output=*)
                output="${1#--output=}"
                ;;
        esac
        shift
    done
    printf '%s\n' "$output"
}

nros_generate_rust_global_signature() {
    local nros="$1"
    local ros_distro="${ROS_DISTRO:-humble}"

    printf 'nros=%s\n' "$(realpath "$nros")"
    sha1sum "$nros" 2>/dev/null || true
    printf 'ros_distro=%s\n' "$ros_distro"
    printf 'ament_prefix_path=%s\n' "${AMENT_PREFIX_PATH:-}"
    printf 'colcon_prefix_path=%s\n' "${COLCON_PREFIX_PATH:-}"

    local prefixes=()
    if [ -n "${AMENT_PREFIX_PATH:-}" ]; then
        IFS=':' read -r -a prefixes <<< "$AMENT_PREFIX_PATH"
    fi
    if [ -d "/opt/ros/$ros_distro" ]; then
        prefixes+=("/opt/ros/$ros_distro")
    fi

    local prefix
    for prefix in "${prefixes[@]}"; do
        [ -d "$prefix/share" ] || continue
        printf 'interface_prefix=%s\n' "$(realpath "$prefix")"
        find "$prefix/share" \
            \( -path '*/msg/*.msg' -o -path '*/srv/*.srv' -o -path '*/action/*.action' -o -name package.xml \) \
            -type f -print0 2>/dev/null \
            | sort -z \
            | xargs -0 sha1sum 2>/dev/null || true
    done
}

nros_generate_rust_signature() {
    local dir="$1"
    local nros="$2"
    shift 2

    if [ "${NROS_GENERATE_RUST_GLOBAL_SIGNATURE_NROS:-}" != "$nros" ]; then
        NROS_GENERATE_RUST_GLOBAL_SIGNATURE="$(nros_generate_rust_global_signature "$nros")"
        NROS_GENERATE_RUST_GLOBAL_SIGNATURE_NROS="$nros"
    fi

    printf '%s\n' "$NROS_GENERATE_RUST_GLOBAL_SIGNATURE"
    printf 'package_dir=%s\n' "$(realpath "$dir")"
    printf 'args=%q\n' "$@"

    find "$dir" \
        \( -path '*/target' -o -path '*/generated' \) -prune -o \
        \( -name package.xml -o -path '*/msg/*.msg' -o -path '*/srv/*.srv' -o -path '*/action/*.action' \) \
        -type f -print0 2>/dev/null \
        | sort -z \
        | xargs -0 sha1sum 2>/dev/null || true
}

nros_generate_rust_if_needed() {
    local dir="$1"
    local nros="$2"
    shift 2
    nros="$(realpath "$nros")"

    local output
    output="$(nros_generate_rust_output_dir "$@")"
    local output_dir="$dir/$output"
    local stamp="$output_dir/.nros-generate-rust.sig"

    if [ "${NROS_GENERATE_RUST_GLOBAL_SIGNATURE_NROS:-}" != "$nros" ]; then
        NROS_GENERATE_RUST_GLOBAL_SIGNATURE="$(nros_generate_rust_global_signature "$nros")"
        NROS_GENERATE_RUST_GLOBAL_SIGNATURE_NROS="$nros"
    fi

    local desired
    desired="$(nros_generate_rust_signature "$dir" "$nros" "$@")"

    if [ "${NROS_GENERATE_BINDINGS_FORCE:-0}" = "1" ] || [ ! -f "$stamp" ] || [ ! -d "$output_dir" ] || ! find "$output_dir" -mindepth 2 -name Cargo.toml -print -quit 2>/dev/null | grep -q . || [ "$(cat "$stamp")" != "$desired" ]; then
        echo "  generate-rust: $dir"
        ( cd "$dir" && "$nros" generate-rust --force "$@" )
        mkdir -p "$output_dir"
        printf '%s\n' "$desired" > "$stamp"
    else
        echo "  skip generate-rust: $dir"
    fi
}
