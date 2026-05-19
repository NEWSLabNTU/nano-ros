#!/usr/bin/env bash
# Phase 118.B.3 — collapse one native/cpp/<rmw>/<case> sibling into
# native/cpp/<case>. C++ side has zenoh + dds today (no xrce dir);
# the canonical add_subdirectory + -DNROS_RMW glue handles both.
#
# Usage: tmp/collapse-native-cpp-case.sh <case>
set -euo pipefail

case_name="${1:?usage: $0 <case>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
src="${root}/examples/native/cpp/zenoh/${case_name}"
dst="${root}/examples/native/cpp/${case_name}"

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
[ -f "$src/README.md" ] && cp "$src/README.md" "$dst/"

exe_target=$(awk '/^add_executable\(/{
    sub(/^add_executable\(/, "");
    print $1; exit
}' "$src/CMakeLists.txt")
exe_target="${exe_target%)}"
[ -z "$exe_target" ] && exe_target="cpp_${case_name//-/_}"

link_libs=$(awk -v exe="$exe_target" '
    BEGIN { capture=0 }
    $0 ~ "target_link_libraries\\(" exe " PRIVATE" { capture=1; next }
    capture {
        if (match($0, /\)/)) {
            sub(/\)/, "", $0)
            print $0
            exit
        }
        print $0
    }
' "$src/CMakeLists.txt")

cat > "$dst/CMakeLists.txt" <<CMAKE
cmake_minimum_required(VERSION 3.22)
project(${exe_target} LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Phase 118.B.3 — collapsed-shape C++ ${case_name}. RMW selected at
# cmake configure time via \`-DNROS_RMW=<rmw>\` (default zenoh).
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|dds|xrce|cyclonedds) — selects the backend linked into this example.")
set(NANO_ROS_RMW "\${NROS_RMW}")
add_subdirectory("\${CMAKE_CURRENT_SOURCE_DIR}/../../../.." nano_ros)

nros_generate_interfaces(builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
nros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)
$(grep -q "example_interfaces" "$src/CMakeLists.txt" && echo "nros_generate_interfaces(example_interfaces DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)")
$(grep -q "action_msgs" "$src/CMakeLists.txt" && echo "nros_generate_interfaces(action_msgs DEPENDENCIES builtin_interfaces LANGUAGE CPP SKIP_INSTALL)")
$(grep -q "unique_identifier_msgs" "$src/CMakeLists.txt" && echo "nros_generate_interfaces(unique_identifier_msgs LANGUAGE CPP SKIP_INSTALL)")

add_executable(${exe_target} src/main.cpp)
target_link_libraries(${exe_target} PRIVATE
${link_libs})
nros_platform_link_app(${exe_target})
CMAKE

cat > "$dst/.gitignore" <<'IGN'
/build/
/build-zenoh/
/build-dds/
/build-xrce/
/build-cyclonedds/
IGN

echo "collapsed: $dst (exe=${exe_target})"
