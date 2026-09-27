#!/bin/bash
# tests/cmake-probe-workspace-caps-tests.sh -- issue 1470
#
# THE PROBE AND THE REAL BUILD MUST AGREE ABOUT WHETHER A TYPE IS BOUNDED.
#
# Field caps (`nros-codegen.toml`, RFC-0033) are the normal way to bound a ROS
# message for an embedded target: the `.msg` belongs to someone else, so a
# consumer cannot add `string<=64` to it, and a buffer cannot be sized from a
# bound that does not exist. The metadata probe compiles the USER's sources, so
# it must compile them with the user's caps -- and it did not: the same type
# generated a BOUNDED header in the board build and an UNBOUNDED one in the
# probe, in one workspace, at the same moment, and the node's own TU then
# refused to compile on the poison template
# (`NROS_UNBOUNDED__<type>__field_<member>`). Every node in the workspace went
# `unprobeable` and the workspace had no source metadata at all.
#
# WHY THE TWO DISAGREED
#
# The Rust lane discovers `nros-codegen.toml` by walking up from the package's
# SOURCE directory (`cargo_nano_ros::generate_from_package_xml`), which reaches
# the workspace root. The CMake lane walked up from the codegen OUTPUT
# directory, which is inside the build tree -- so a workspace-scope config was
# unreachable from a CMake build in principle, whatever the build was for. One
# question, two answers; `_nros_codegen_config_chain` makes it one.
#
# WHAT IS ASSERTED
#
#   B. THE REFUSAL STILL WORKS (run first). With NO caps anywhere, the
#      generated header states no bound, carries the poison token by name, and
#      the node's TU FAILS to compile on it. That assertion is load-bearing: a
#      "fix" that bounded everything by default would pass case A and ship
#      buffers sized from nothing.
#
#   A. THE GATE. With the workspace's `nros-codegen.toml` capping the field, the
#      SAME sources in the SAME probe-shaped project produce a header with no
#      poison and a stated TX/RX bound, and the node's TU compiles.
#
# Cases A and B differ by exactly one file -- the workspace's caps -- and use
# separate build trees, so neither can launder the other's artifacts.
#
# PRECONDITIONS ARE HARD FAILURES. This script never prints-and-returns: a green
# here is read as "the probe honours the workspace's caps". Skipping belongs in
# the `just` recipe's check ledger, which is a different claim from "it passed".
#
# Usage: ./tests/cmake-probe-workspace-caps-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# `nros_grep_q` -- 0 match / 1 no-match / exit 2 when grep could not run, so a
# tool failure never becomes a finding (issue 0726). Load-bearing here in BOTH
# directions: this script reads the ABSENCE of a poison token as "the cap
# arrived", so a grep that failed to fork would report a bound that does not
# exist.
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

FAILURES=0
CHECKS=0

fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}

check() {
    CHECKS=$((CHECKS + 1))
}

for _t in cmake c++; do
    if ! command -v "$_t" >/dev/null 2>&1; then
        fail "$_t is not on PATH -- this test cannot report a verdict without it"
        exit 1
    fi
done

NROS_BIN="$PROJECT_ROOT/packages/cli/target/release/nros"
if [ ! -x "$NROS_BIN" ]; then
    fail "in-tree CLI not built at $NROS_BIN -- run \`just setup-cli\` (codegen has no other producer)"
    exit 1
fi
# Same env discipline as tests/cmake-probe-shared-types-tests.sh: a DIFFERENT
# checkout's `nros` on PATH, or an inherited `nano_ros_ROOT`, would configure
# this tree with that checkout's codegen (issues 0363, 1280).
export PATH="$PROJECT_ROOT/packages/cli/target/release:$PATH"
export nano_ros_ROOT="$PROJECT_ROOT"
export NANO_ROS_ROOT="$PROJECT_ROOT"
export NROS_WORKSPACE="$PROJECT_ROOT"
export NROS_REPO_DIR="$PROJECT_ROOT"

init_test_tmpdir "nros-probe-workspace-caps"
trap 'cleanup_test_tmpdir' EXIT

WS="$TEST_TMPDIR/ws"
CAPS_FILE="$WS/nros-codegen.toml"

# ---------------------------------------------------------------------------
# The workspace under test -- the island's shape at the smallest size that
# carries it.
#
# `island_msgs/Report` embeds `std_msgs/Header`, whose `frame_id` is an
# UNBOUNDED string. That is the island's `autoware_vehicle_msgs/VelocityReport`
# exactly: a workspace-local interface package over a STOCK one, where the
# unbounded member is inside the stock type. The cap therefore cannot live in a
# `.msg` this workspace owns, which is the whole reason `nros-codegen.toml`
# exists.
#
# `pkg_a` subscribes to it, so its TU instantiates `rx_size_bound<Report>` --
# the point at which the poison template is evaluated and a missing bound stops
# being a comment.
# ---------------------------------------------------------------------------
write_workspace() {
    mkdir -p "$WS/src/island_msgs/msg"
    cat > "$WS/src/island_msgs/package.xml" <<'EOF'
<?xml version="1.0"?>
<package format="3">
  <name>island_msgs</name>
  <version>0.1.0</version>
  <description>Workspace-local interface package embedding a stock type with an unbounded string.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <buildtool_depend>ament_cmake</buildtool_depend>
  <build_depend>rosidl_default_generators</build_depend>
  <depend>std_msgs</depend>
  <member_of_group>rosidl_interface_packages</member_of_group>
  <export><build_type>ament_cmake</build_type></export>
</package>
EOF
    cat > "$WS/src/island_msgs/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.22)
project(island_msgs)
find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)
find_package(std_msgs REQUIRED)
rosidl_generate_interfaces(${PROJECT_NAME}
    msg/Report.msg
    DEPENDENCIES std_msgs
)
ament_package()
EOF
    printf 'std_msgs/Header header\nfloat64 speed\n' > "$WS/src/island_msgs/msg/Report.msg"

    local dir="$WS/src/pkg_a"
    mkdir -p "$dir/include/pkg_a" "$dir/src"
    cat > "$dir/package.xml" <<'EOF'
<?xml version="1.0"?>
<package format="3">
  <name>pkg_a</name>
  <version>0.1.0</version>
  <description>C++ node package that asks for its message's size bound.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <depend>island_msgs</depend>
  <export><build_type>nros_cmake</build_type></export>
</package>
EOF
    cat > "$dir/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.22)
project(pkg_a VERSION 0.1.0 LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
find_package(nano_ros REQUIRED)
find_package(island_msgs REQUIRED)
nano_ros_auto_add_library(pkg_a_lib STATIC src/NodeA.cpp)
nros_components_register_node(pkg_a_lib
    PLUGIN pkg_a::NodeA
    EXECUTABLE node_a
    SHAPE configure
)
if(TARGET island_msgs__nano_ros_cpp)
    target_link_libraries(pkg_a_lib PUBLIC island_msgs__nano_ros_cpp)
endif()
EOF
    cat > "$dir/include/pkg_a/NodeA.hpp" <<'EOF'
#pragma once

#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "island_msgs.hpp"

namespace pkg_a {

class NodeA {
    int recv_ = 0;

    void on_msg(const ::island_msgs::msg::Report& msg);

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace pkg_a
EOF
    cat > "$dir/src/NodeA.cpp" <<'EOF'
#include "pkg_a/NodeA.hpp"

namespace pkg_a {

void NodeA::on_msg(const ::island_msgs::msg::Report& msg) {
    recv_ += static_cast<int>(msg.speed);
}

::rclcpp::Result NodeA::configure(::rclcpp::Node& node) {
    return ::nros::bind_subscription<::island_msgs::msg::Report, NodeA, &NodeA::on_msg>(
        node, "/report", this);
}

} // namespace pkg_a
EOF
}

# The workspace's caps, in the RFC-0033 workspace scope: at the workspace root,
# shared by every member. `frame_id` is inside a STOCK type, so this is the only
# place it can be bounded at all.
write_caps() {
    cat > "$CAPS_FILE" <<'EOF'
[fields]
"std_msgs/Header.frame_id" = { cap = 64, mode = "inline" }
EOF
}

# ---------------------------------------------------------------------------
# The probe project, in the shape `render_probe_cmakelists()` emits (phase-313 +
# issue 0662 + issue 1469). The probe EXECUTABLE is left out -- it adds the
# runtime link and changes nothing about which caps codegen reads -- but
# `pkg_a_lib` is built for real, because the poison is a COMPILER error and
# nothing short of a compile can report on it.
# ---------------------------------------------------------------------------
render_probe() {
    local pd="$1"
    mkdir -p "$pd"
    {
        echo 'cmake_minimum_required(VERSION 3.22)'
        echo 'project(nros_metadata_probes LANGUAGES C CXX)'
        echo 'set(CMAKE_CXX_STANDARD 14)'
        echo 'set(CMAKE_CXX_STANDARD_REQUIRED ON)'
        echo 'set(NROS_EXTRA_CPP_FEATURES "metadata-mode")'
        echo 'set(NANO_ROS_GEN_CACHE_DIR "${CMAKE_BINARY_DIR}/nros-codegen")'
        echo "set(NROS_INTERFACE_SEARCH_PATH \"$WS/src\")"
        echo 'find_package(nano_ros REQUIRED)'
        echo 'nros_workspace_interfaces()'
        echo "add_subdirectory($WS/src/pkg_a pkg_a)"
    } > "$pd/CMakeLists.txt"
}

# The generated C++ header for `island_msgs/Report`, wherever the shared codegen
# dir put it. Printed rather than assumed: a case that cannot find its own
# artifact is a case that did not run.
report_header() {
    find "$1" -path '*nano_ros_cpp/island_msgs/msg/island_msgs_msg_report.hpp' | head -n 1
}

POISON='NROS_UNBOUNDED__island_msgs_msg_report__field_header_frame_id'

# Configure + generate + compile one case. $1 = case label, $2 = dirs prefix.
# Sets CASE_HEADER and CASE_COMPILE_LOG; returns the compile's exit status.
run_case() {
    local label="$1" prefix="$2"
    local probe="$TEST_TMPDIR/probe-$prefix" build="$TEST_TMPDIR/build-$prefix"
    render_probe "$probe"
    if ! cmake -S "$probe" -B "$build" -DCMAKE_PREFIX_PATH="$PROJECT_ROOT" \
            > "$TEST_TMPDIR/configure-$prefix.log" 2>&1; then
        fail "[$label] probe configure failed"
        tail -n 30 "$TEST_TMPDIR/configure-$prefix.log"
        exit 1
    fi
    # Codegen only, then the node's own TU. Two steps so a missing header is
    # distinguishable from a refused compile.
    if ! cmake --build "$build" --target island_msgs__nano_ros_cpp_gen \
            > "$TEST_TMPDIR/codegen-$prefix.log" 2>&1; then
        fail "[$label] codegen for island_msgs failed"
        tail -n 30 "$TEST_TMPDIR/codegen-$prefix.log"
        exit 1
    fi
    CASE_HEADER="$(report_header "$build")"
    if [ -z "$CASE_HEADER" ]; then
        fail "[$label] no generated header for island_msgs/Report -- the case did not run"
        exit 1
    fi
    CASE_COMPILE_LOG="$TEST_TMPDIR/compile-$prefix.log"
    cmake --build "$build" --target pkg_a_lib > "$CASE_COMPILE_LOG" 2>&1
}

write_workspace

# ---------------------------------------------------------------------------
# B. The refusal still works (run FIRST -- case A's green says nothing until an
#    unbounded field has been shown to still be refused, by name).
# ---------------------------------------------------------------------------
log_header "B. with no caps, an unbounded member is still refused by name"
rm -f "$CAPS_FILE"
run_case "no caps" uncapped
B_RC=$?

check
if nros_grep_q "$POISON" "$CASE_HEADER"; then
    log_success "the header states no bound and carries the poison token"
else
    fail "an uncapped string produced no poison token in $(basename "$CASE_HEADER") -- the refusal is gone"
fi
check
if [ "$B_RC" -ne 0 ] && nros_grep_q "$POISON" "$CASE_COMPILE_LOG"; then
    log_success "and the node's TU refuses to compile, naming $POISON"
else
    fail "expected the node's TU to fail on $POISON (exit $B_RC); see $CASE_COMPILE_LOG"
    grep -n 'error' "$CASE_COMPILE_LOG" | head -n 5
fi

# ---------------------------------------------------------------------------
# A. The gate: the probe honours the workspace's caps.
# ---------------------------------------------------------------------------
log_header "A. the probe honours the workspace's nros-codegen.toml"
write_caps
run_case "workspace caps" capped
A_RC=$?

check
if nros_grep_q 'NROS_UNBOUNDED__' "$CASE_HEADER"; then
    fail "the capped field STILL reads as unbounded in the probe's own header:"
    grep -n 'NROS_UNBOUNDED__' "$CASE_HEADER" | head -n 4 | sed 's/^/        /'
else
    log_success "no NROS_UNBOUNDED marker in the probe's generated header"
fi
check
if nros_grep_q 'static constexpr size_t RX_MAX_SERIALIZED_SIZE' "$CASE_HEADER"; then
    log_success "and it states a bound: $(grep -o 'RX_MAX_SERIALIZED_SIZE = [0-9]*' "$CASE_HEADER" | head -n 1)"
else
    fail "the header states no RX_MAX_SERIALIZED_SIZE -- the cap did not reach codegen"
fi
check
if [ "$A_RC" -eq 0 ]; then
    log_success "and the node's own TU compiles"
else
    fail "the node's TU failed to compile with the workspace's caps in place:"
    grep -n 'error' "$CASE_COMPILE_LOG" | head -n 5 | sed 's/^/        /'
fi

echo
if [ "$FAILURES" -eq 0 ]; then
    log_success "all $CHECKS checks passed"
    exit 0
fi
log_error "$FAILURES of $CHECKS checks failed"
exit 1
