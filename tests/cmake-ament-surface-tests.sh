#!/bin/bash
# tests/cmake-ament-surface-tests.sh -- phase-488 W3
#
# `cmake/NanoRosAmentSurface.cmake` against the CMakeLists shape a stock ROS 2
# C++ package has (Autoware's `autoware_external_cmd_selector` @0.45.0 was the
# port that found each of these). Configured in a real `project()` with a stand-in
# `NanoRos::NanoRosCpp`, because every assertion is about what the verbs DECLARE,
# not about the runtime they link:
#
#   1. `rclcpp_components_register_node(... EXECUTABLE ...)` configures (it
#      failed on EVERY call: "Unknown argument to GENERATE subcommand") and
#      generates a main that forward-declares the PLUGIN class in its
#      namespaces and calls the factory the registration macro defines;
#   2. a PLUGIN at global scope works too, and one that is not a qualified C++
#      name is refused by name;
#   3. `ament_auto_add_library(... SHARED ...)` builds STATIC (one image is one
#      binary: a SHARED library linked a second copy of the runtime);
#   4. `ament_auto_add_*` give a target the package's `include/` (PUBLIC on a
#      library, PRIVATE on an executable) and every `package.xml` dependency
#      nano-ros answers for, and REPORT one it does not, without a lookup;
#   5. a message package installed under `AMENT_PREFIX_PATH` gets a Find stub
#      routing `find_package()` to the nano-ros resolver; one nano-ros ships a
#      module for does not;
#   6. `find_package(diagnostic_updater)` carries `diagnostic_msgs`, which its
#      header includes (a template that linked the messages by hand hid this);
#   7. an ament package on a host that declares no `NROS_FEATURES` gets
#      `param_services` (upstream runs the parameter services in every node);
#      a declaration, even an empty one, wins, and a non-ament project or an
#      embedded platform gets nothing;
#   8. with `param_services` in `NROS_FEATURES`, an ament target (and
#      `rclcpp::rclcpp`) is compiled with NROS_AMENT_START_PARAMETER_SERVICES,
#      so its node constructor serves the parameters; without it, it is not.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

expect() {
    if [[ "$3" == *"$2"* ]]; then echo "  ok    $1"; else
        echo "  FAIL  $1 -- wanted '$2' in:"; sed 's/^/        /' <<<"$3"; fail=1; fi
}
refute() {
    if [[ "$3" != *"$2"* ]]; then echo "  ok    $1"; else
        echo "  FAIL  $1 -- did not want '$2' in:"; sed 's/^/        /' <<<"$3"; fail=1; fi
}

# A package with a component, a tool executable and its own header.
mk() {
    local d="$WORK/$1"
    mkdir -p "$d/src" "$d/include/demo_pkg"
    : > "$d/src/comp.cpp"
    : > "$d/src/tool.cpp"
    : > "$d/include/demo_pkg/comp.hpp"
    cat > "$d/package.xml" <<'XML'
<?xml version="1.0"?>
<package format="3">
  <name>demo_pkg</name>
  <version>0.0.0</version>
  <description>phase-488 ament surface fixture</description>
  <maintainer email="none@example.com">none</maintainer>
  <license>Apache-2.0</license>
  <buildtool_depend>ament_cmake_auto</buildtool_depend>
  <depend>rclcpp</depend>
  <depend>rclcpp_components</depend>
  <build_depend>not_provided_by_nano_ros</build_depend>
</package>
XML
    cat > "$d/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(demo_pkg CXX)
add_library(nros_runtime_stand_in INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS nros_runtime_stand_in)
include("$ROOT/cmake/NanoRosPackageXml.cmake")
include("$ROOT/cmake/NanoRosAmentSurface.cmake")
find_package(ament_cmake_auto REQUIRED)
ament_auto_find_build_dependencies()
ament_auto_add_library(comp SHARED src/comp.cpp)
ament_auto_add_executable(tool src/tool.cpp)
rclcpp_components_register_node(comp PLUGIN "$2" EXECUTABLE comp_node)
get_target_property(_t comp TYPE)
get_target_property(_ci comp INTERFACE_INCLUDE_DIRECTORIES)
get_target_property(_ti tool INCLUDE_DIRECTORIES)
get_target_property(_tii tool INTERFACE_INCLUDE_DIRECTORIES)
get_target_property(_tl tool LINK_LIBRARIES)
message(STATUS "comp TYPE=[\${_t}]")
message(STATUS "comp IFACE_INC=[\${_ci}]")
message(STATUS "tool INC=[\${_ti}] IFACE_INC=[\${_tii}]")
message(STATUS "tool LINK=[\${_tl}]")
message(STATUS "found=[\${demo_pkg_FOUND_BUILD_DEPENDS}]")
ament_auto_package()
CML
}
run() { cmake -S "$WORK/$1" -B "$WORK/$1/b" "${@:2}" 2>&1; }

mk ok "demo_ns::inner::Comp"
rc=0
out="$(AMENT_PREFIX_PATH= run ok)" || rc=$?
expect "1. register_node with EXECUTABLE configures" "Generating done" "$out"
main="$WORK/ok/b/nros_ament_main/comp_node_main.cpp"
if [ -f "$main" ]; then
    body="$(cat "$main")"
    expect "1. main forward-declares the class in its namespaces" \
        "namespace demo_ns { namespace inner { class Comp; } }" "$body"
    expect "1. main calls the factory the macro defines" \
        "nros_ament_component_detail::make_component<demo_ns::inner::Comp>(" "$body"
else
    echo "  FAIL  1. no generated main at $main"; fail=1
fi
expect "3. a SHARED component builds STATIC" "comp TYPE=[STATIC_LIBRARY]" "$out"
expect "4. a library exports the package include/" "comp IFACE_INC=[$WORK/ok/include]" "$out"
expect "4. an executable gets include/ privately" "tool INC=[$WORK/ok/include] IFACE_INC=[_tii-NOTFOUND]" "$out"
expect "4. package.xml deps nano-ros answers are found" "found=[rclcpp;rclcpp_components]" "$out"
expect "4. ...and linked" "rclcpp::rclcpp" "$out"
expect "4. a dep nano-ros does not provide is reported" \
    'depends on `not_provided_by_nano_ros`, which nano-ros does not provide' "$out"

mk flat "Comp"
AMENT_PREFIX_PATH= run flat >/dev/null || true
expect "2. a global-scope PLUGIN is declared at global scope" \
    "class Comp; " "$(cat "$WORK/flat/b/nros_ament_main/comp_node_main.cpp" 2>&1)"
mk bad "demo-ns::Comp"
rc=0
out="$(AMENT_PREFIX_PATH= run bad)" || rc=$?
expect "2. a PLUGIN that is not a C++ name is refused" "PLUGIN 'demo-ns::Comp' is not a" "$out"
[ "$rc" -ne 0 ] || { echo "  FAIL  2. the refusal did not fail the configure"; fail=1; }

# 5. An AMENT install with one message package nano-ros ships no module for, one
#    it does (std_msgs), and a share dir that is not a message package.
ament="$WORK/ament"
mkdir -p "$ament/share/vendor_msgs/msg" "$ament/share/std_msgs/msg" "$ament/share/vendor_tool/config"
mk am "demo_ns::Comp"
out="$(AMENT_PREFIX_PATH="$ament" run am)" || true
stubs="$WORK/am/b/nros-find-stubs"
if [ -f "$stubs/Findvendor_msgs.cmake" ]; then
    expect "5. an AMENT message package gets a stub naming the resolver" \
        "_nros_find_ros_msg_package(vendor_msgs)" "$(cat "$stubs/Findvendor_msgs.cmake")"
else
    echo "  FAIL  5. no Findvendor_msgs.cmake under $stubs"; fail=1
fi
[ ! -e "$stubs/Findstd_msgs.cmake" ] && echo "  ok    5. a package nano-ros ships a module for is skipped" \
    || { echo "  FAIL  5. std_msgs got a stub over the shipped module"; fail=1; }
[ ! -e "$stubs/Findvendor_tool.cmake" ] && echo "  ok    5. a non-message share dir gets none" \
    || { echo "  FAIL  5. vendor_tool got a stub"; fail=1; }

# 6. A stand-in message target, so the find module need not run codegen.
d="$WORK/du"; mkdir -p "$d"
cat > "$d/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(du CXX)
add_library(nros_runtime_stand_in INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS nros_runtime_stand_in)
add_library(diagnostic_msgs__nano_ros_cpp INTERFACE)
include("$ROOT/cmake/NanoRosAmentSurface.cmake")
find_package(diagnostic_updater REQUIRED)
find_package(diagnostic_updater REQUIRED)
get_target_property(_l nros-diagnostic-updater INTERFACE_LINK_LIBRARIES)
message(STATUS "du LINK=[\${_l}]")
CML
out="$(AMENT_PREFIX_PATH= cmake -S "$d" -B "$d/b" 2>&1)" || true
expect "6. diagnostic_updater links diagnostic_msgs" ";diagnostic_msgs__nano_ros_cpp;::@]" "$out"
refute "6. ...once, however often it is found" \
    "diagnostic_msgs__nano_ros_cpp;diagnostic_msgs__nano_ros_cpp" "$out"

# 7. The default is asked from the nano-ros root's directory, so the probe
#    calls it from a subdirectory standing in for that root.
mk_caps() {
    local d="$WORK/caps-$1"
    mkdir -p "$d/nros"
    printf '%s\n' "$2" > "$d/package.xml"
    cat > "$d/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(caps NONE)
$3
add_subdirectory(nros)
CML
    cat > "$d/nros/CMakeLists.txt" <<CML
include("$ROOT/cmake/NanoRosCapabilities.cmake")
nros_ament_port_default_features(_d)
message(STATUS "default=[\${_d}]")
CML
    cmake -S "$d" -B "$d/b" 2>&1
}
ament_xml='<package><buildtool_depend>ament_cmake_auto</buildtool_depend></package>'
expect "7. an ament package on posix gets param_services" "default=[param_services]" \
    "$(mk_caps ament "$ament_xml" 'set(NROS_PLATFORM posix)')"
mk_caps umbrella '<workspace/>' 'set(NROS_PLATFORM posix)' >/dev/null
mkdir -p "$WORK/caps-umbrella/src/pkg"
printf '%s\n' "$ament_xml" > "$WORK/caps-umbrella/src/pkg/package.xml"
rm -rf "$WORK/caps-umbrella/b"
expect "7. so does an umbrella over a colcon-layout src/" "default=[param_services]" \
    "$(cmake -S "$WORK/caps-umbrella" -B "$WORK/caps-umbrella/b" 2>&1)"
expect "7. a declared (empty) NROS_FEATURES wins" "default=[]" \
    "$(mk_caps declared "$ament_xml" 'set(NROS_PLATFORM posix)
set(NROS_FEATURES "")')"
expect "7. an embedded platform gets nothing" "default=[]" \
    "$(mk_caps rtos "$ament_xml" 'set(NROS_PLATFORM freertos)')"
expect "7. a project that is not an ament package gets nothing" "default=[]" \
    "$(mk_caps plain '<package><buildtool_depend>catkin</buildtool_depend></package>' 'set(NROS_PLATFORM posix)')"

# 8.
mk_ps() {
    local d="$WORK/ps-$1"; mkdir -p "$d"; : > "$d/a.cpp"
    cat > "$d/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(ps CXX)
$2
add_library(nros_runtime_stand_in INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS nros_runtime_stand_in)
include("$ROOT/cmake/NanoRosPackageXml.cmake")
include("$ROOT/cmake/NanoRosAmentSurface.cmake")
find_package(rclcpp REQUIRED)
ament_auto_add_executable(exe a.cpp)
get_target_property(_d exe COMPILE_DEFINITIONS)
get_target_property(_r rclcpp::rclcpp INTERFACE_COMPILE_DEFINITIONS)
message(STATUS "exe DEFS=[\${_d}] rclcpp DEFS=[\${_r}]")
CML
    AMENT_PREFIX_PATH= cmake -S "$d" -B "$d/b" 2>&1
}
out="$(mk_ps on 'set(NROS_FEATURES param_services)')"
expect "8. an ament target serves parameters when the runtime has them" \
    "exe DEFS=[NROS_CPP_STD=1;NROS_AMENT_START_PARAMETER_SERVICES=1]" "$out"
expect "8. ...and so does rclcpp::rclcpp" \
    "rclcpp DEFS=[NROS_CPP_STD=1;NROS_AMENT_START_PARAMETER_SERVICES=1]" "$out"
out="$(mk_ps off 'set(NROS_FEATURES "")')"
refute "8. not without param_services" "NROS_AMENT_START_PARAMETER_SERVICES" "$out"

[ "$fail" -eq 0 ] && echo "cmake-ament-surface: OK" || { echo "cmake-ament-surface: FAILED"; exit 1; }
