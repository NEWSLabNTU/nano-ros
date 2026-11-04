# shellcheck shell=bash
# One reader of a west workspace's `.west/config` `[manifest]` section, shared
# by `scripts/zephyr/unbind-manifest-project.sh` and
# `scripts/zephyr/sync-workspace-manifest.sh` (issue 2249).
#
#   west_config_field <config-file> <key>    # prints the value, rc 1 if no file
west_config_field() {
    local cfg="$1" key="$2"
    [ -f "$cfg" ] || return 1
    awk -v key="$key" '
        /^\[/ { in_manifest = ($0 ~ /^\[manifest\][[:space:]]*$/); next }
        in_manifest && $1 == key { sub(/^[^=]*=[[:space:]]*/, ""); print; exit }
    ' "$cfg"
}
