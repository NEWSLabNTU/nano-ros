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
#      a declaration, even an empty one, wins, and a non-ament project gets
#      nothing; an embedded platform gets `lifecycle` alone, and only for a
#      package that depends on `rclcpp_lifecycle`;
#   8. with `param_services` in `NROS_FEATURES`, an ament target (and
#      `rclcpp::rclcpp`) is compiled with NROS_AMENT_START_PARAMETER_SERVICES,
#      so its node constructor serves the parameters; without it, it is not.
#   9. the plain-CMake component shape (ros2/demos `composition` @jazzy):
#      `add_library(<c> SHARED)` builds STATIC (an IMPORTED SHARED library is
#      left alone); `rclcpp_components_register_nodes` and
#      `ament_environment_hooks` are accepted; `ament_target_dependencies`
#      claims no `target_link_libraries` signature, so a plain AND a keyword
#      link beside it both configure, a library passes its dependencies on and
#      `INTERFACE` passes them on only; and a plain `add_executable` that takes
#      rclcpp that way gets the platform link pass, once.
#  10. the multi-package shape (Nav2's `nav2_common` / `nav2_util` /
#      `nav2_velocity_smoother`): a library package of the workspace is found
#      by its sibling through a workspace Find stub, as `<pkg>::<pkg>` over what
#      it exported, with its CONFIG_EXTRAS run (the macro it defines exists) and
#      `<pkg>_TARGETS` set; a package found before it is configured is refused
#      by name; `find_package()` of an INSTALLED package nano-ros has no module
#      for is refused naming the package and the file, not as an unknown ament
#      command; `rclcpp_lifecycle`, `rclcpp_action`, `ament_cmake_python` and
#      `rcutils` resolve; and a message package's `<pkg>_TARGETS` is set
#      (`rcl_interfaces` included, which was a no-op module).
#  11. on a road with no platform link command (Zephyr), the link pass the
#      ament verbs ask for is a no-op, not a deferred call to a missing one;
#  12. the C/C++ FreeRTOS carrier's app stack is 64 KiB unless the configure
#      sets `NROS_FREERTOS_APP_STACK_KB` (issue 2070).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

# issue 1850 — this checkout's CLI, or the explicit $NROS_CLI the gate passes.
# The message resolver asks `nros profile`, and without this the cmake modules
# fall through to whatever `nros` the host has installed, which may predate it.
# shellcheck source=scripts/build/cargo.sh
source "$ROOT/scripts/build/cargo.sh"
NROS_BIN="$(nros_cli_bin --checkout-only)" || NROS_BIN=""
if [ -z "$NROS_BIN" ]; then
    echo "  FAIL  in-tree CLI not built -- run \`just setup-cli\`"
    exit 1
fi
export NROS_CLI="$NROS_BIN"
export PATH="$(dirname "$NROS_BIN"):$PATH"

expect() {
    if [[ "$3" == *"$2"* ]]; then echo "  ok    $1"; else
        echo "  FAIL  $1 -- wanted '$2' in:"; sed 's/^/        /' <<<"$3"; fail=1; fi
}
refute() {
    if [[ "$3" != *"$2"* ]]; then echo "  ok    $1"; else
        echo "  FAIL  $1 -- did not want '$2' in:"; sed 's/^/        /' <<<"$3"; fail=1; fi
}

# A package with a component, a tool executable and its own header.
# Every case but 7 includes NanoRosAmentSurface.cmake, whose message-package
# resolver needs this checkout's `nros`. A pristine clone (`git worktree add`)
# has none, so those cases are a REPORTED skip there, never a failure and never
# a pass (issues 1373, 1043); case 7 needs no CLI and always runs.
# shellcheck source=scripts/build/cargo.sh
source "$ROOT/scripts/build/cargo.sh"
have_cli=1
nros_cli_bin --checkout-only >/dev/null 2>&1 || have_cli=0
if [ "$have_cli" -eq 0 ]; then
    # shellcheck source=scripts/build/check-skip.sh
    . "$ROOT/scripts/build/check-skip.sh"
    nros_check_skip cmake-ament-surface \
        "cases 1-6 and 8 NOT VERIFIED -- this checkout built no \`nros\` (run \`just setup-cli\`)"
fi

if [ "$have_cli" -eq 1 ]; then
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

fi  # have_cli (cases 1-6)

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
lc_xml='<package><buildtool_depend>ament_cmake</buildtool_depend><depend>rclcpp_lifecycle</depend></package>'
expect "7. a package depending on rclcpp_lifecycle also gets lifecycle" \
    "default=[param_services;lifecycle]" "$(mk_caps lifecycle "$lc_xml" 'set(NROS_PLATFORM posix)')"
expect "7. an embedded package depending on rclcpp_lifecycle gets lifecycle alone" \
    "default=[lifecycle]" "$(mk_caps rtoslc "$lc_xml" 'set(NROS_PLATFORM freertos)')"
expect "7. ...not one that does not" "default=[param_services]" \
    "$(mk_caps nolc '<package><buildtool_depend>ament_cmake</buildtool_depend><depend>rclcpp</depend></package>' 'set(NROS_PLATFORM posix)')"
expect "7. a project that is not an ament package gets nothing" "default=[]" \
    "$(mk_caps plain '<package><buildtool_depend>catkin</buildtool_depend></package>' 'set(NROS_PLATFORM posix)')"

if [ "$have_cli" -eq 1 ]; then
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

fi  # have_cli (case 8)

if [ "$have_cli" -eq 1 ]; then
# 9.
d="$WORK/comp"; mkdir -p "$d"; : > "$d/a.cpp"; : > "$d/m.cpp"
cat > "$d/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(comp CXX)
add_library(nros_runtime_stand_in INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS nros_runtime_stand_in)
# Stand-in for the platform pass: records each target it is run for.
function(nros_platform_link_app t)
    set_property(GLOBAL APPEND PROPERTY LINKED_APPS \${t})
endfunction()
include("$ROOT/cmake/NanoRosAmentSurface.cmake")
include("$ROOT/cmake/NanoRosAmentSurface.cmake")
find_package(rclcpp REQUIRED)
find_package(rclcpp_components REQUIRED)
add_library(talker SHARED a.cpp)
add_library(prebuilt SHARED IMPORTED)
ament_target_dependencies(talker rclcpp rclcpp_components)
rclcpp_components_register_nodes(talker "demo::Talker")
ament_environment_hooks("hook.sh")
add_executable(plain m.cpp)
target_link_libraries(plain talker)
ament_target_dependencies(plain rclcpp)
ament_target_dependencies(plain rclcpp)
add_executable(keyword m.cpp)
target_link_libraries(keyword PRIVATE talker)
ament_target_dependencies(keyword rclcpp)
add_library(iface INTERFACE)
ament_target_dependencies(iface INTERFACE rclcpp)
foreach(_t talker prebuilt)
    get_target_property(_ty \${_t} TYPE)
    message(STATUS "\${_t} TYPE=[\${_ty}]")
endforeach()
get_target_property(_l talker LINK_LIBRARIES)
get_target_property(_il talker INTERFACE_LINK_LIBRARIES)
get_target_property(_pl plain LINK_LIBRARIES)
get_target_property(_fl iface INTERFACE_LINK_LIBRARIES)
message(STATUS "talker LINK=[\${_l}] IFACE=[\${_il}]")
message(STATUS "plain LINK=[\${_pl}]")
message(STATUS "iface IFACE=[\${_fl}]")
function(_report)
    get_property(_a GLOBAL PROPERTY LINKED_APPS)
    message(STATUS "linked=[\${_a}]")
endfunction()
cmake_language(DEFER CALL _report)
CML
rc=0
out="$(AMENT_PREFIX_PATH= cmake -S "$d" -B "$d/b" 2>&1)" || rc=$?
expect "9. the composition shape configures" "Generating done" "$out"
expect "9. add_library(SHARED) builds STATIC" "talker TYPE=[STATIC_LIBRARY]" "$out"
expect "9. ...and says so" "add_library(talker SHARED) builds STATIC" "$out"
expect "9. an IMPORTED SHARED library is left alone" "prebuilt TYPE=[SHARED_LIBRARY]" "$out"
expect "9. a library takes its dependencies and passes them on" \
    "talker LINK=[rclcpp::rclcpp;rclcpp_components::rclcpp_components] IFACE=[rclcpp::rclcpp;rclcpp_components::rclcpp_components]" "$out"
expect "9. a plain link beside it keeps both" "plain LINK=[talker;rclcpp::rclcpp;rclcpp::rclcpp]" "$out"
expect "9. INTERFACE passes them on only" "iface IFACE=[rclcpp::rclcpp]" "$out"
expect "9. each executable taking rclcpp gets the platform pass, once" "linked=[plain;keyword]" "$out"

fi  # have_cli (case 9)

if [ "$have_cli" -eq 1 ]; then
# 10.
w="$WORK/ws"; mkdir -p "$w/src/libpkg/include/libpkg" "$w/src/libpkg/cmake" "$w/src/user" \
    "$w/ament/share/upstream_lib/cmake"
: > "$w/src/libpkg/a.cpp"; : > "$w/src/user/u.cpp"
for p in libpkg user; do
    printf '<package format="3"><name>%s</name><buildtool_depend>ament_cmake</buildtool_depend></package>\n' "$p" \
        > "$w/src/$p/package.xml"
done
cat > "$w/src/libpkg/cmake/libpkg_macro.cmake" <<'CML'
macro(libpkg_setup)
    set(LIBPKG_SETUP_RAN TRUE)
endmacro()
CML
cat > "$w/src/libpkg/libpkg-extras.cmake" <<'CML'
include("${libpkg_DIR}/libpkg_macro.cmake")
CML
cat > "$w/src/libpkg/CMakeLists.txt" <<'CML'
project(libpkg CXX)
find_package(ament_cmake REQUIRED)
find_package(ament_cmake_python REQUIRED)
ament_python_install_package(libpkg)
add_library(libpkg_core SHARED a.cpp)
ament_export_libraries(libpkg_core)
ament_package(CONFIG_EXTRAS "libpkg-extras.cmake")
CML
cat > "$w/src/user/CMakeLists.txt" <<'CML'
project(user CXX)
find_package(ament_cmake REQUIRED)
find_package(libpkg REQUIRED)
libpkg_setup()
find_package(rclcpp_lifecycle REQUIRED)
find_package(rclcpp_action REQUIRED)
find_package(rcutils REQUIRED)
add_library(user_lib STATIC u.cpp)
ament_target_dependencies(user_lib libpkg rclcpp_lifecycle)
get_target_property(_l user_lib LINK_LIBRARIES)
get_target_property(_li libpkg::libpkg INTERFACE_LINK_LIBRARIES)
get_target_property(_ii libpkg::libpkg INTERFACE_INCLUDE_DIRECTORIES)
message(STATUS "setup=[${LIBPKG_SETUP_RAN}] TARGETS=[${libpkg_TARGETS}]")
message(STATUS "user LINK=[${_l}]")
message(STATUS "libpkg IFACE=[${_li}] INC=[${_ii}]")
if(TARGET libpkg::libpkg_core)
    message(STATUS "aliased")
endif()
foreach(_p rclcpp_lifecycle rclcpp_action)
    if(TARGET ${_p}::${_p})
        message(STATUS "${_p}: target")
    endif()
endforeach()
CML
cat > "$w/ament/share/upstream_lib/cmake/upstream_libConfig.cmake" <<'CML'
include("${CMAKE_CURRENT_LIST_DIR}/ament_cmake_export_include_directories-extras.cmake")
CML
cat > "$w/ament/share/upstream_lib/cmake/ament_cmake_export_include_directories-extras.cmake" <<'CML'
normalize_path(_d "${CMAKE_CURRENT_LIST_DIR}")
CML
mkws() {
    cat > "$w/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(ws CXX)
add_library(nros_runtime_stand_in INTERFACE)
add_library(NanoRos::NanoRosCpp ALIAS nros_runtime_stand_in)
set(NROS_INTERFACE_SEARCH_PATH "\${CMAKE_SOURCE_DIR}/src")
include("$ROOT/cmake/NanoRosAmentSurface.cmake")
# Stand-in for a message package codegen already wired.
add_library(vendor_msgs__nano_ros_cpp INTERFACE)
add_library(rcl_interfaces__nano_ros_cpp INTERFACE)
include("$ROOT/cmake/find/_NrosFindRosMsgPackage.cmake")
_nros_find_ros_msg_package(vendor_msgs)
find_package(rcl_interfaces REQUIRED)
message(STATUS "msg TARGETS=[\${vendor_msgs_TARGETS}] rcl=[\${rcl_interfaces_TARGETS}]")
$1
CML
    rm -rf "$w/b"
    AMENT_PREFIX_PATH= cmake -S "$w" -B "$w/b" -DCMAKE_PREFIX_PATH="$w/ament" 2>&1
}
out="$(mkws 'add_subdirectory(src/libpkg)
add_subdirectory(src/user)')" || true
expect "10. the multi-package workspace configures" "Generating done" "$out"
expect "10. a sibling finds the library package and its CONFIG_EXTRAS ran" \
    "setup=[TRUE] TARGETS=[libpkg::libpkg]" "$out"
expect "10. ament_target_dependencies links it" "user LINK=[libpkg::libpkg;rclcpp_lifecycle::rclcpp_lifecycle]" "$out"
expect "10. <pkg>::<pkg> carries the exported library and the package include/" \
    "libpkg IFACE=[libpkg_core] INC=[$w/src/libpkg/include]" "$out"
expect "10. an exported library is aliased <pkg>::<target>" "aliased" "$out"
expect "10. rclcpp_lifecycle resolves to a target" "rclcpp_lifecycle: target" "$out"
expect "10. rclcpp_action resolves to a target" "rclcpp_action: target" "$out"
expect "10. a message package states <pkg>_TARGETS, rcl_interfaces too" \
    "msg TARGETS=[vendor_msgs::vendor_msgs] rcl=[rcl_interfaces::rcl_interfaces]" "$out"
out="$(mkws 'add_subdirectory(src/user)
add_subdirectory(src/libpkg)')" || true
expect "10. a package found before it is configured is refused by name" \
    "find_package(libpkg) asks for a package of this workspace" "$out"
out="$(mkws 'find_package(upstream_lib REQUIRED)')" || true
expect "10. an installed package with no module is refused naming it" \
    "find_package(upstream_lib) loaded the INSTALLED ROS 2 package's" "$out"
refute "10. ...not as an unknown ament command" 'Unknown CMake command "normalize_path"' "$out"

fi  # have_cli (case 10)

# 11. phase-518 -- the link pass on a road with no platform link command
#     (Zephyr: NanoRosEntry defines the deferred wrapper, nothing defines
#     `nros_platform_link_app`). It used to schedule a call to the missing
#     command, and the configure died at the end of the directory.
d="$WORK/link-no-platform"; mkdir -p "$d"; : > "$d/a.cpp"
cat > "$d/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(lnp CXX)
function(nros_platform_link_app_deferred target)
    cmake_language(EVAL CODE
        "cmake_language(DEFER CALL nros_platform_link_app_once [[\${target}]])")
endfunction()
include("$ROOT/cmake/NanoRosAmentTargetDeps.cmake")
add_executable(img a.cpp)
_nros_ament_link_app(img)
message(STATUS "link pass asked")
CML
out="$(cmake -S "$d" -B "$d/b" 2>&1)" || true
expect "11. with no platform link command the link pass is a no-op" "Generating done" "$out"
refute "11. ...and schedules no call to a missing command" "Unknown CMake command" "$out"

# 12. phase-518 -- the C/C++ FreeRTOS carrier's app stack, from
#     NROS_FREERTOS_APP_STACK_KB (issue 2070), 64 KiB unset.
d="$WORK/app-stack"; mkdir -p "$d"
cat > "$d/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(appstack NONE)
include("$ROOT/cmake/NanoRosBoardNet.cmake")
_nros_freertos_entry_app_stack(_def)
set(NROS_FREERTOS_APP_STACK_KB 192)
_nros_freertos_entry_app_stack(_set)
message(STATUS "stack default=[\${_def}] set=[\${_set}]")
CML
out="$(cmake -S "$d" -B "$d/b" 2>&1)" || true
expect "12. the app stack defaults to 64 KiB and follows NROS_FREERTOS_APP_STACK_KB" \
    "stack default=[65536] set=[196608]" "$out"

[ "$fail" -eq 0 ] && echo "cmake-ament-surface: OK" || { echo "cmake-ament-surface: FAILED"; exit 1; }
