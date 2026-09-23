#!/bin/bash
# tests/cmake-probe-shared-types-tests.sh -- issue 1469
#
# THE SCALING RULE. A workspace with ONE C++ node package probes. A workspace
# with TWO that share a message dependency did not, and the failure was
# proportional to the number of SHARED types rather than to anything the author
# wrote: the aggregating FFI crate `include!`d the shared type twice and rustc
# refused with E0428, so every component in the workspace went `unprobeable`
# and the workspace had no source metadata at all.
#
# WHY TWO PACKAGES ARE NEEDED AND WHY TWO CONFIGURES ARE TOO
#
# Within one configure the generators are idempotent: the second consumer of a
# shared type finds `<pkg>__nano_ros_cpp` already defined and emits nothing. The
# generation SITE, though, is whichever consuming package reached the type
# first, and `_NROS_PKG_<pkg>_GENERATED_RS_FILES` is a `CACHE INTERNAL` entry --
# it outlives the configure. Re-configure the same build dir with a DIFFERENT
# set of packages (which is exactly what the probe's drop-and-retry loop does,
# and what a sync does when one component's unprobeable marker clears) and the
# site moves while the cached closures of the packages that were NOT reached
# this pass keep naming the old one. The aggregating crate then takes one dep's
# closure from this pass and another's from the last, and gets two paths for one
# type. `include!` is textual, so that is two definitions of every item in the
# file.
#
# So the reproduction is: two C++ node packages that share a message dependency,
# probed twice into one build dir with a different one of them live each time.
# That is this script's case A, and it is the shape the Autoware Safety Island
# hit with `builtin_interfaces/msg/{Time,Duration}` -- eight E0428s.
#
# WHAT IS ASSERTED
#
#   A. Two C++ node packages sharing one message dependency, configured across
#      two passes with different live sets, produce FFI crates in which every
#      generated type is `include!`d exactly once. This is the gate.
#
#   B. The detector detects. A hand-written `lib.rs` carrying the island's own
#      duplicate pair IS reported -- otherwise case A's green says nothing more
#      than "the grep found nothing".
#
#   C. One generation site per type. With the fix the probe's shape is the
#      board build's: a shared type exists at exactly ONE path under the
#      workspace-shared codegen dir, not once per consuming package.
#
# PRECONDITIONS ARE HARD FAILURES. This script never prints-and-returns: a green
# here is read as "the probe's include list is sound". Skipping belongs in the
# `just` recipe's check ledger, which is a different claim from "it passed".
#
# Usage: ./tests/cmake-probe-shared-types-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

FAILURES=0
CHECKS=0

fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}

check() {
    CHECKS=$((CHECKS + 1))
}

for _t in cmake; do
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
# The cmake modules resolve `nros` off PATH before anything else, and a
# DIFFERENT checkout's binary on PATH would configure this tree with that
# checkout's codegen (the shape issue 0363 guards). Put this one first.
export PATH="$PROJECT_ROOT/packages/cli/target/release:$PATH"
# Same reason for the cmake package search: `nano_ros_ROOT` in the environment
# points `find_package(nano_ros)` at whichever checkout was last activated.
export nano_ros_ROOT="$PROJECT_ROOT"
export NANO_ROS_ROOT="$PROJECT_ROOT"
export NROS_WORKSPACE="$PROJECT_ROOT"
# ... and the checkout the CLI reports against, for the same reason: an
# inherited `NROS_REPO_DIR` from another activation makes `abi_guard` compare
# this tree against that one (issue 1280).
export NROS_REPO_DIR="$PROJECT_ROOT"

init_test_tmpdir "nros-probe-shared-types"
trap 'cleanup_test_tmpdir' EXIT

WS="$TEST_TMPDIR/ws"

# ---------------------------------------------------------------------------
# The workspace under test -- the island's own shape, at the smallest size that
# still carries it.
#
# `shared_msgs` and `combo_msgs` are WORKSPACE-LOCAL message packages that
# depend on STOCK ones (`builtin_interfaces`, `std_msgs`). That pairing is what
# makes the generation site observable: a workspace-local interface package is
# re-generated on EVERY pass (`nros_workspace_interfaces()` runs before any node
# package), while a stock package is generated only when a LIVE consumer reaches
# it -- so the two halves of one closure can come from different passes. The
# island's `nav_msgs` -> `std_msgs` / `geometry_msgs` is this shape.
#
# `combo_msgs` has TWO dependencies, which is the last ingredient: the duplicate
# needs two dep closures that disagree about where a shared transitive type
# lives, one refreshed this pass and one carried over from the last.
#
# `pkg_a` and `pkg_b` are the two C++ node packages, and they share `shared_msgs`
# (`pkg_a` through `combo_msgs`) and `builtin_interfaces` under it.
# ---------------------------------------------------------------------------
write_workspace() {
    mkdir -p "$WS/src/combo_msgs/msg"
    cat > "$WS/src/combo_msgs/package.xml" <<'EOF'
<?xml version="1.0"?>
<package format="3">
  <name>combo_msgs</name>
  <version>0.1.0</version>
  <description>Workspace-local interface package over one local and one stock dependency.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <buildtool_depend>ament_cmake</buildtool_depend>
  <build_depend>rosidl_default_generators</build_depend>
  <depend>shared_msgs</depend>
  <depend>std_msgs</depend>
  <member_of_group>rosidl_interface_packages</member_of_group>
  <export><build_type>ament_cmake</build_type></export>
</package>
EOF
    cat > "$WS/src/combo_msgs/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.22)
project(combo_msgs)
find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)
find_package(shared_msgs REQUIRED)
find_package(std_msgs REQUIRED)
rosidl_generate_interfaces(${PROJECT_NAME}
    msg/Combined.msg
    DEPENDENCIES shared_msgs std_msgs
)
ament_package()
EOF
    printf 'shared_msgs/Stamped stamped\nstd_msgs/Int32 count\n' > "$WS/src/combo_msgs/msg/Combined.msg"

    mkdir -p "$WS/src/shared_msgs/msg"
    cat > "$WS/src/shared_msgs/package.xml" <<'EOF'
<?xml version="1.0"?>
<package format="3">
  <name>shared_msgs</name>
  <version>0.1.0</version>
  <description>Workspace-local interface package over a stock dependency.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <buildtool_depend>ament_cmake</buildtool_depend>
  <build_depend>rosidl_default_generators</build_depend>
  <depend>builtin_interfaces</depend>
  <member_of_group>rosidl_interface_packages</member_of_group>
  <export><build_type>ament_cmake</build_type></export>
</package>
EOF
    cat > "$WS/src/shared_msgs/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.22)
project(shared_msgs)
find_package(ament_cmake REQUIRED)
find_package(rosidl_default_generators REQUIRED)
find_package(builtin_interfaces REQUIRED)
rosidl_generate_interfaces(${PROJECT_NAME}
    msg/Stamped.msg
    DEPENDENCIES builtin_interfaces
)
ament_package()
EOF
    printf 'builtin_interfaces/Time stamp\nint32 value\n' > "$WS/src/shared_msgs/msg/Stamped.msg"

    # pkg_a comes in through `combo_msgs`, pkg_b directly through
    # `shared_msgs`. Asymmetric on purpose: it is what leaves `std_msgs`
    # un-reached in the pass where only pkg_b is live, so its cached closure
    # stays a pass behind the site the next pass generates at.
    local p msgpkg msgtype field
    for p in a b; do
        if [ "$p" = "a" ]; then
            msgpkg=combo_msgs; msgtype=Combined; field="msg.count.data"
        else
            msgpkg=shared_msgs; msgtype=Stamped; field="msg.value"
        fi
        local dir="$WS/src/pkg_$p"
        mkdir -p "$dir/include/pkg_$p" "$dir/src"
        cat > "$dir/package.xml" <<EOF
<?xml version="1.0"?>
<package format="3">
  <name>pkg_$p</name>
  <version>0.1.0</version>
  <description>C++ node package sharing an interface closure with its sibling.</description>
  <maintainer email="dev@example.com">dev</maintainer>
  <license>Apache-2.0</license>
  <depend>$msgpkg</depend>
  <export><build_type>nros_cmake</build_type></export>
</package>
EOF
        cat > "$dir/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.22)
project(pkg_$p VERSION 0.1.0 LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
find_package(nano_ros REQUIRED)
find_package($msgpkg REQUIRED)
nano_ros_auto_add_library(pkg_${p}_lib STATIC src/Node$p.cpp)
nros_components_register_node(pkg_${p}_lib
    PLUGIN pkg_$p::Node$p
    EXECUTABLE node_$p
    SHAPE configure
)
if(TARGET ${msgpkg}__nano_ros_cpp)
    target_link_libraries(pkg_${p}_lib PUBLIC ${msgpkg}__nano_ros_cpp)
endif()
EOF
        cat > "$dir/include/pkg_$p/Node$p.hpp" <<EOF
#pragma once

#include <cstdint>

#include <nros/component.hpp>
#include <nros/nros.hpp>

#include "$msgpkg.hpp"

namespace pkg_$p {

class Node$p {
    int recv_ = 0;

    void on_msg(const ::${msgpkg}::msg::$msgtype& msg);

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace pkg_$p
EOF
        cat > "$dir/src/Node$p.cpp" <<EOF
#include "pkg_$p/Node$p.hpp"

namespace pkg_$p {

void Node$p::on_msg(const ::${msgpkg}::msg::$msgtype& msg) {
    recv_ += static_cast<int>($field);
}

::rclcpp::Result Node$p::configure(::rclcpp::Node& node) {
    return ::nros::bind_subscription<::${msgpkg}::msg::$msgtype, Node$p, &Node$p::on_msg>(
        node, "/stamped_$p", this);
}

} // namespace pkg_$p
EOF
    done
}

# ---------------------------------------------------------------------------
# The probe project, in the shape `render_probe_cmakelists()` emits (phase-313 +
# issue 0662 + issue 1469). Only the probe EXECUTABLES are left out: they add
# the runtime link and change nothing about the include list under test.
#
# `$1` is the probe dir, the rest are the LIVE packages for this pass.
# ---------------------------------------------------------------------------
render_probe() {
    local pd="$1"; shift
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
        local p
        for p in "$@"; do
            echo "add_subdirectory($WS/src/$p $p)"
        done
    } > "$pd/CMakeLists.txt"
}

# Every `<pkg>/<kind>/<stem>.rs` an FFI crate's lib.rs includes more than once.
# That tail is what the file DEFINES; the leading path is only where the copy
# sits, which is the distinction the bug turned on.
duplicate_types_in() {
    grep -oE 'nano_ros_cpp/[a-z0-9_]+/(msg|srv|action)/[a-z0-9_]+\.rs' "$1" \
        | sort | uniq -d
}

# ---------------------------------------------------------------------------
# B. The detector detects (run FIRST -- a green case A is worth nothing until
#    this one has shown that a duplicate would have been seen).
# ---------------------------------------------------------------------------
log_header "B. a duplicated type IS reported"
CANARY="$TEST_TMPDIR/canary_lib.rs"
cat > "$CANARY" <<'EOF'
include!("../../../pkg_a/nano_ros_cpp/builtin_interfaces/msg/builtin_interfaces_msg_time_types.rs");
include!("../../../pkg_b/nano_ros_cpp/builtin_interfaces/msg/builtin_interfaces_msg_time_types.rs");
include!("../../nano_ros_cpp/shared_msgs/msg/shared_msgs_msg_stamped_types.rs");
EOF
check
CANARY_DUPS="$(duplicate_types_in "$CANARY")"
if [ "$CANARY_DUPS" = "nano_ros_cpp/builtin_interfaces/msg/builtin_interfaces_msg_time_types.rs" ]; then
    log_success "the island's duplicate pair is reported by the detector"
else
    fail "detector missed a planted duplicate (got: ${CANARY_DUPS:-<nothing>})"
    exit 1
fi

# ---------------------------------------------------------------------------
# A. The scaling rule.
# ---------------------------------------------------------------------------
log_header "A. two C++ node packages sharing one message dependency"
write_workspace
PROBE="$TEST_TMPDIR/probe"
BUILD="$TEST_TMPDIR/build"
CFG_LOG="$TEST_TMPDIR/configure.log"

# The live set CHANGES between the passes, which is what moves the generation
# site. Both orders are exercised: the second pass must not depend on which of
# the two siblings went first.
for live in pkg_b pkg_a pkg_b; do
    render_probe "$PROBE" "$live"
    check
    if ! cmake -S "$PROBE" -B "$BUILD" -DCMAKE_PREFIX_PATH="$PROJECT_ROOT" \
            > "$CFG_LOG" 2>&1; then
        fail "probe configure failed with [$live] live"
        tail -n 30 "$CFG_LOG"
        exit 1
    fi
    log_success "configured with [$live] live"

    seen_crate=0
    for lib in $(find "$BUILD" -name lib.rs -path '*nano_ros_cpp_ffi_*'); do
        seen_crate=1
        check
        dups="$(duplicate_types_in "$lib")"
        if [ -n "$dups" ]; then
            fail "after [$live]: ${lib#"$BUILD"/} includes a type more than once:"
            echo "$dups" | sed 's/^/        /'
        fi
    done
    if [ "$seen_crate" -eq 0 ]; then
        fail "after [$live]: no FFI crate was generated -- the case did not run"
        exit 1
    fi
done
if [ "$FAILURES" -eq 0 ]; then
    log_success "every generated type is include!d exactly once, across all passes"
fi

# ---------------------------------------------------------------------------
# C. One generation site per type -- the board build's shape.
# ---------------------------------------------------------------------------
log_header "C. a shared type is generated once, not once per consumer"
check
# The generated .rs files are BUILD-time outputs, so at configure time the
# observable site is the output DIRECTORY the generator makes for the package
# (`file(MAKE_DIRECTORY)` in `nros_generate_interfaces`). One per shared type is
# the property; one per consuming package was the bug.
SITES="$(find "$BUILD" -type d -path '*/nano_ros_cpp/builtin_interfaces' | wc -l)"
CRATES="$(find "$BUILD" -name lib.rs -path '*nano_ros_cpp_ffi_builtin_interfaces*' | wc -l)"
if [ "$CRATES" -eq 1 ]; then
    log_success "builtin_interfaces has exactly one FFI crate for the whole probe"
else
    fail "expected ONE builtin_interfaces FFI crate for the probe, found $CRATES"
fi
check
if [ "$SITES" -eq 1 ]; then
    log_success "and exactly one generation site for its types ($SITES)"
else
    fail "a shared type is emitted at $SITES sites -- the probe is back to one copy per consumer"
fi

echo
if [ "$FAILURES" -eq 0 ]; then
    log_success "all $CHECKS checks passed"
    exit 0
fi
log_error "$FAILURES of $CHECKS checks failed"
exit 1
