#!/usr/bin/env bash
#
# Phase 23.2 — Assemble the distributable Arduino Library zip.
#
# Inputs (must exist before running this script):
#   arduino/nros/library.properties     — version, name, architectures
#   arduino/nros/src/<arch>/libnanoros.a — precompiled archives
#                                          (built by build-libnanoros.sh)
#   arduino/nros/src/nros_arduino.{h,cpp}
#   arduino/nros/src/nros/              — C API headers
#   arduino/nros/src/<pkg>/             — pre-generated message headers
#
# Output:
#   build/arduino/nano-ros-arduino-v<VERSION>.zip
#
# The version string is read from `library.properties`. CI uses a
# release tag to override via `ARDUINO_LIB_VERSION=<tag>`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ARDUINO_LIB_ROOT="$NANO_ROS_ROOT/arduino/nros"
OUT_DIR="$NANO_ROS_ROOT/build/arduino"

if [[ ! -d "$ARDUINO_LIB_ROOT" ]]; then
    echo "arduino/nros not present at $ARDUINO_LIB_ROOT" >&2
    exit 2
fi

version_from_props() {
    awk -F= '/^version=/ {print $2; exit}' "$ARDUINO_LIB_ROOT/library.properties"
}
VERSION="${ARDUINO_LIB_VERSION:-$(version_from_props)}"
if [[ -z "$VERSION" ]]; then
    echo "could not read version from library.properties" >&2
    exit 2
fi

mkdir -p "$OUT_DIR"
zip_name="nano-ros-arduino-v${VERSION}.zip"
zip_path="$OUT_DIR/$zip_name"

# Sanity: at least one libnanoros.a must exist or the zip is useless.
shopt -s nullglob
found_any=0
for a in "$ARDUINO_LIB_ROOT"/src/*/libnanoros.a; do
    found_any=1
    break
done
shopt -u nullglob
if [[ "$found_any" -eq 0 ]]; then
    echo "no libnanoros.a found under $ARDUINO_LIB_ROOT/src/<arch>/ —" >&2
    echo "run scripts/arduino/build-libnanoros.sh first" >&2
    exit 3
fi

rm -f "$zip_path"

# zip from one directory up so the archive entries start with `nros/`,
# matching what Arduino IDE expects after extraction into
# `~/Arduino/libraries/`.
(
    cd "$NANO_ROS_ROOT/arduino"
    zip -r -q "$zip_path" nros \
        -x 'nros/src/*/build/*' \
        -x 'nros/.gitignore'
)

sz=$(du -h "$zip_path" | cut -f1)
echo "wrote $zip_path ($sz)"
