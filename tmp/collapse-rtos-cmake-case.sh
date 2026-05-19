#!/usr/bin/env bash
# Phase 118.B — collapse one examples/<plat>/<lang>/<rmw>/<case>
# (lang=c|cpp, cmake build) sibling into examples/<plat>/<lang>/<case>.
# RMW selection moves to cmake `-DNROS_RMW=<rmw>` cache var.
#
# Usage: tmp/collapse-rtos-cmake-case.sh <plat> <lang> <case>
#
# Examples:
#   tmp/collapse-rtos-cmake-case.sh qemu-arm-freertos c talker
#   tmp/collapse-rtos-cmake-case.sh qemu-arm-nuttx cpp listener
set -euo pipefail

plat="${1:?usage: $0 <plat> <lang> <case>}"
lang="${2:?usage: $0 <plat> <lang> <case>}"
case_name="${3:?usage: $0 <plat> <lang> <case>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
src="${root}/examples/${plat}/${lang}/zenoh/${case_name}"
dst="${root}/examples/${plat}/${lang}/${case_name}"

if [ ! -d "$src" ]; then
    echo "missing source: $src" >&2
    exit 1
fi
if [ -d "$dst" ]; then
    echo "already exists: $dst — skipping"
    exit 0
fi

mkdir -p "$dst"
cp -r "$src/src" "$dst/"
[ -f "$src/config.toml" ] && cp "$src/config.toml" "$dst/"
[ -f "$src/package.xml" ] && cp "$src/package.xml" "$dst/"
[ -f "$src/README.md" ] && cp "$src/README.md" "$dst/"

# Read the source CMakeLists once; transform it minimally:
#   - drop the hardcoded `set(NANO_ROS_RMW <rmw>)` line
#   - replace with the NROS_RMW cache var pattern
#   - fix the add_subdirectory("../../../../..") path depth 5→4
cmake_in="$src/CMakeLists.txt"
cmake_out="$dst/CMakeLists.txt"

python3 - "$cmake_in" "$cmake_out" "$case_name" <<'PY'
import re
import sys
from pathlib import Path

in_path = Path(sys.argv[1])
out_path = Path(sys.argv[2])
case_name = sys.argv[3]
text = in_path.read_text()

# Drop the hardcoded `set(NANO_ROS_RMW   zenoh)` and replace with
# the `NROS_RMW` cache-var pattern right after the platform set.
text = re.sub(
    r'^(set\(NANO_ROS_PLATFORM[^\n]*\)\s*\n)set\(NANO_ROS_RMW\s+[^\n)]*\)',
    r'\1set(NROS_RMW "zenoh" CACHE STRING\n'
    r'    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")\n'
    r'set(NANO_ROS_RMW "${NROS_RMW}")',
    text,
    count=1,
    flags=re.MULTILINE,
)

# 5-segment path → 4-segment path (plat/lang/rmw/case → plat/lang/case)
text = text.replace('../../../../..', '../../../..')

out_path.write_text(text)
PY

# .gitignore mirrors the rest of the cmake-side examples.
cat > "$dst/.gitignore" <<'IGN'
/build/
/build-zenoh/
/build-dds/
/build-xrce/
/build-cyclonedds/
IGN

echo "collapsed: ${dst} (lang=${lang})"
