#!/usr/bin/env bash
# Phase 211.J — re-bake the committed `record-*.json` files after editing
# the launch files. The parser needs absolute `<include file=…>` paths, so
# rewrite the `$(dirname)` placeholder to the launch dir first, then bake.
#
# The parser is the one `nros-launch-resolve` links (issue 1413 retired the
# standalone `play_launch_parser` binary this used to need); `--record -`
# prints the raw parse. Requires `nros` on PATH (`source ./activate.sh`).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
RESOLVE="$(nros locate tool nros-launch-resolve)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
LAUNCH="$TMP/launch"
mkdir -p "$LAUNCH"
cp "$HERE"/src/demo_inc/launch/*.launch.xml "$HERE"/demo_inc_bringup/launch/system.launch.xml "$LAUNCH/"
sed -i "s|\$(dirname)|$LAUNCH|g" "$LAUNCH"/*.launch.xml

# 17-level chain — generate the 16 intermediates next to deep_entry.
for i in $(seq 0 16); do
    next=$((i + 1))
    if [ "$i" -lt 16 ]; then
        printf '<launch>\n  <include file="%s/lvl_%s.launch.xml" />\n</launch>\n' \
            "$LAUNCH" "$next" > "$LAUNCH/lvl_$i.launch.xml"
    else
        cp "$LAUNCH/leaf.launch.xml" "$LAUNCH/lvl_$i.launch.xml"
    fi
done

for scenario in chain cycle deep; do
    case "$scenario" in
        chain) entry="system.launch.xml" ;;
        cycle) entry="cycle_entry.launch.xml" ;;
        deep)  entry="deep_entry.launch.xml" ;;
    esac
    if "$RESOLVE" "$LAUNCH/$entry" --record - > "$TMP/record.json"; then
        # `origin.path` is this run's tmp dir, so drop it; keys are sorted so
        # a re-bake of an unchanged tree reproduces the committed bytes.
        python3 -c '
import json, sys
r = json.load(open(sys.argv[1]))
for s in r.get("scopes", []):
    (s.get("origin") or {}).pop("path", None)
open(sys.argv[2], "w").write(json.dumps(r, indent=2, sort_keys=True))
' "$TMP/record.json" "$HERE/record-$scenario.json"
        echo "baked record-$scenario.json"
    else
        echo "FAIL: no record for $scenario" >&2; exit 1
    fi
done
