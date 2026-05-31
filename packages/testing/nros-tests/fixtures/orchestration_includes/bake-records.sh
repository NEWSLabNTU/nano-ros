#!/usr/bin/env bash
# Phase 211.J — re-bake the committed `record-*.json` files after editing
# the launch files. `play_launch_parser` doesn't expand `$(dirname)`, so
# rewrite the placeholder to the absolute path first, then bake.
#
# Requires `play_launch_parser` and `nros` on PATH.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
LAUNCH="$HERE/src/demo_inc/launch"
TMP="$(mktemp -d)"; cp -r "$HERE"/* "$TMP/"
sed -i "s|\$(dirname)|$LAUNCH|g" "$TMP"/src/demo_inc/launch/*.launch.xml

# 17-level chain — generate the 16 intermediates next to deep_entry.
for i in $(seq 0 16); do
    next=$((i + 1))
    if [ "$i" -lt 16 ]; then
        cat > "$TMP/src/demo_inc/launch/lvl_$i.launch.xml" <<INNER
<launch>
  <include file="$LAUNCH/lvl_$next.launch.xml" />
</launch>
INNER
    else
        cp "$TMP/src/demo_inc/launch/leaf.launch.xml" "$TMP/src/demo_inc/launch/lvl_$i.launch.xml"
    fi
done

for scenario in chain cycle deep; do
    case "$scenario" in
        chain) entry="system.launch.xml" ;;
        cycle) entry="cycle_entry.launch.xml" ;;
        deep)  entry="deep_entry.launch.xml" ;;
    esac
    out="$(mktemp -d)"
    (cd "$TMP" && nros plan demo_inc "src/demo_inc/launch/$entry" \
        --workspace . --nros-toml nros.toml --out-dir "$out" || true)
    if [ -f "$out/record.json" ]; then
        cp "$out/record.json" "$HERE/record-$scenario.json"
        echo "baked record-$scenario.json"
    else
        echo "WARN: no record.json for $scenario (parser likely errored)"
    fi
done
