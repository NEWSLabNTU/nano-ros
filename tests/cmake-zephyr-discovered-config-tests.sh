#!/bin/bash
# tests/cmake-zephyr-discovered-config-tests.sh -- issue 2374
#
# `zephyr/cmake/nros_discovered_config.cmake` keeps Zephyr's application config
# discovery live across re-configures of ONE build dir. Asserted in a real
# `project()` that runs the module's two halves around a stand-in for Zephyr's
# `configuration_files.cmake` discovery block (same rule: discover only while
# the variable is undefined in every scope `zephyr_get` reads, then cache what
# was found) and `kconfig.cmake`'s "File not found":
#
#   1. a fresh configure discovers prj.conf;
#   2. a boards/<board>.conf ADDED later is picked up by `cmake <dir>`;
#   3. ... and by a plain BUILD (the directory is a configure input);
#   4. the fragment DELETED later: `cmake <dir>` recovers (was: File not found);
#   5. an overlay added/deleted behaves the same (DTC_OVERLAY_FILE);
#   6. a `-DCONF_FILE=...` is a choice: it suppresses discovery, then and on the
#      next reconfigure;
#   7. an application's own `set(CONF_FILE ...)` is a choice too;
#   8. a dir configured BEFORE the hook (no marker) whose cached list names a
#      deleted fragment recovers -- issue 2374's evidence dirs;
#   9. ... while a marker-less RELATIVE `-DCONF_FILE=prj.conf` stays a choice;
#  10. a configure that stopped after discovery and before the record still
#      rediscovers next time;
#  11. a cache EDIT after discovery (ccmake) is a choice.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOOK="${NROS_DISCOVERED_CONFIG_HOOK:-$ROOT/zephyr/cmake/nros_discovered_config.cmake}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

GEN=()
if command -v ninja >/dev/null 2>&1; then GEN=(-G Ninja); fi

mk() {
    local d="$WORK/$1"
    mkdir -p "$d/cfg/boards"
    echo "CONFIG_A=y" > "$d/cfg/prj.conf"
    cat > "$d/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(dc NONE)
$2
# Stand-in for Zephyr's zephyr_get(<var> SYSBUILD LOCAL): cache, ENV, local.
function(zephyr_get _v)
    if(DEFINED CACHE{\${_v}})
        set(\${_v} "\$CACHE{\${_v}}" PARENT_SCOPE)
    elseif(DEFINED ENV{\${_v}})
        set(\${_v} "\$ENV{\${_v}}" PARENT_SCOPE)
    endif()
endfunction()
if(NOT DEFINED CACHE{APPLICATION_CONFIG_DIR})
    set(APPLICATION_CONFIG_DIR "\${CMAKE_CURRENT_SOURCE_DIR}/cfg")
else()
    set(APPLICATION_CONFIG_DIR "\$CACHE{APPLICATION_CONFIG_DIR}")
endif()

include("$HOOK")
nros_discovered_config_pre()          # zephyr/modules/modules.cmake

# ---- configuration_files.cmake (3.7 / 4.4), the discovery rule ----
zephyr_get(CONF_FILE)
if(NOT DEFINED CONF_FILE)
    set(CONF_FILE "\${APPLICATION_CONFIG_DIR}/prj.conf")
    file(GLOB _b "\${APPLICATION_CONFIG_DIR}/boards/*.conf")
    list(SORT _b)
    list(APPEND CONF_FILE \${_b})
endif()
set(APPLICATION_CONFIG_DIR \${APPLICATION_CONFIG_DIR} CACHE PATH "cfg" FORCE)
set(CONF_FILE \${CONF_FILE} CACHE STRING "Zephyr's CONF_FILE")
zephyr_get(DTC_OVERLAY_FILE)
if(NOT DEFINED DTC_OVERLAY_FILE)
    file(GLOB DTC_OVERLAY_FILE "\${APPLICATION_CONFIG_DIR}/boards/*.overlay")
    if(DTC_OVERLAY_FILE STREQUAL "")
        unset(DTC_OVERLAY_FILE)
    endif()
endif()
set(DTC_OVERLAY_FILE \${DTC_OVERLAY_FILE} CACHE STRING "Zephyr's DTC_OVERLAY_FILE")

# ---- kconfig.cmake / dts.cmake: every named file must exist ----
foreach(_f IN LISTS CONF_FILE DTC_OVERLAY_FILE)
    get_filename_component(_abs "\${_f}" ABSOLUTE BASE_DIR "\${APPLICATION_CONFIG_DIR}")
    if(NOT EXISTS "\${_abs}")
        message(FATAL_ERROR "File not found: \${_abs}")
    endif()
endforeach()
if(EXISTS "\${APPLICATION_CONFIG_DIR}/STOP")
    message(FATAL_ERROR "stopped between discovery and the module (case 10)")
endif()

nros_discovered_config_post()         # zephyr/CMakeLists.txt
string(REPLACE "\${APPLICATION_CONFIG_DIR}/" "" _c "\$CACHE{CONF_FILE}")
string(REPLACE "\${APPLICATION_CONFIG_DIR}/" "" _d "\$CACHE{DTC_OVERLAY_FILE}")
message(STATUS "CONF=[\${_c}] DTC=[\${_d}]")
CML
}
cfg() { cmake -S "$WORK/$1" -B "$WORK/$1/b" "${GEN[@]}" "${@:2}" 2>&1 || true; }
recfg() { cmake "$WORK/$1/b" "${@:2}" 2>&1 || true; }
expect() {
    if [[ "$3" == *"$2"* ]]; then echo "  ok    $1"; else
        echo "  FAIL  $1 -- wanted '$2' in:"; sed 's/^/        /' <<<"$3"; fail=1; fi
}

mk live ""
expect "1. a fresh configure discovers prj.conf" "CONF=[prj.conf] DTC=[]" "$(cfg live)"
echo "CONFIG_B=y" > "$WORK/live/cfg/boards/b.conf"
expect "2. an added fragment is picked up by cmake <dir>" \
    "CONF=[prj.conf;boards/b.conf]" "$(recfg live)"
rm "$WORK/live/cfg/boards/b.conf"
expect "4. a deleted fragment: cmake <dir> recovers" "CONF=[prj.conf] DTC=[]" "$(recfg live)"
echo "/ {};" > "$WORK/live/cfg/boards/b.overlay"
expect "5. an added overlay is picked up" "DTC=[boards/b.overlay]" "$(recfg live)"
rm "$WORK/live/cfg/boards/b.overlay"
expect "5. a deleted overlay: cmake <dir> recovers" "CONF=[prj.conf] DTC=[]" "$(recfg live)"

if [ "${#GEN[@]}" -gt 0 ]; then
    sleep 1   # a directory mtime newer than build.ninja, on 1 s filesystems
    echo "CONFIG_C=y" > "$WORK/live/cfg/boards/c.conf"
    out="$(cmake --build "$WORK/live/b" 2>&1 || true)"
    expect "3. an added fragment re-runs cmake on a plain build" "CONF=[prj.conf;boards/c.conf]" "$out"
    sleep 1
    rm "$WORK/live/cfg/boards/c.conf"
    out="$(cmake --build "$WORK/live/b" 2>&1 || true)"
    expect "3. a deleted fragment re-runs cmake on a plain build" "CONF=[prj.conf] DTC=[]" "$out"
else
    echo "  FAIL  3. needs ninja (a directory configure input is a ninja edge)"; fail=1
fi

mk dash ""
echo "CONFIG_B=y" > "$WORK/dash/cfg/boards/b.conf"
expect "6. a -DCONF_FILE suppresses discovery" "CONF=[prj.conf] DTC=[]" \
    "$(cfg dash -DCONF_FILE=prj.conf)"
expect "6. and keeps suppressing it on reconfigure" "CONF=[prj.conf] DTC=[]" "$(recfg dash)"
echo "CONFIG_X=y" > "$WORK/dash/cfg/boards/x.conf"
expect "6. even after a fragment is added" "CONF=[prj.conf] DTC=[]" "$(recfg dash)"

mk dashabs ""
echo "CONFIG_B=y" > "$WORK/dashabs/cfg/boards/b.conf"
expect "6. an ABSOLUTE -DCONF_FILE inside the config dir is a choice too" "CONF=[prj.conf] DTC=[]" \
    "$(cfg dashabs "-DCONF_FILE=$WORK/dashabs/cfg/prj.conf")"
expect "6. and stays one on reconfigure" "CONF=[prj.conf] DTC=[]" "$(recfg dashabs)"

mk setv 'set(CONF_FILE "${CMAKE_CURRENT_SOURCE_DIR}/cfg/prj.conf")'
echo "CONFIG_B=y" > "$WORK/setv/cfg/boards/b.conf"
expect "7. an application's set() is a choice" "CONF=[prj.conf] DTC=[]" "$(cfg setv)"
expect "7. and stays one on reconfigure" "CONF=[prj.conf] DTC=[]" "$(recfg setv)"

mk legacy ""
echo "CONFIG_B=y" > "$WORK/legacy/cfg/boards/b.conf"
cfg legacy >/dev/null
rm "$WORK/legacy/cfg/boards/b.conf"
expect "8. a marker-less dir naming a deleted fragment recovers" "CONF=[prj.conf] DTC=[]" \
    "$(recfg legacy -UNROS_ZEPHYR_DISCOVERED_CONF_FILE -UNROS_ZEPHYR_DISCOVERED_DTC_OVERLAY_FILE)"

mk legrel ""
echo "CONFIG_B=y" > "$WORK/legrel/cfg/boards/b.conf"
cfg legrel -DCONF_FILE=prj.conf >/dev/null
expect "9. a marker-less relative -DCONF_FILE stays a choice" "CONF=[prj.conf] DTC=[]" \
    "$(recfg legrel -UNROS_ZEPHYR_DISCOVERED_CONF_FILE)"

mk legabs ""
echo "CONFIG_B=y" > "$WORK/legabs/cfg/boards/b.conf"
cfg legabs >/dev/null
expect "9. a marker-less ABSOLUTE -DCONF_FILE on this configure stays a choice" \
    "CONF=[prj.conf] DTC=[]" \
    "$(recfg legabs -UNROS_ZEPHYR_DISCOVERED_CONF_FILE "-DCONF_FILE=$WORK/legabs/cfg/prj.conf")"

mk edit ""
echo "CONFIG_B=y" > "$WORK/edit/cfg/boards/b.conf"
cfg edit >/dev/null
sed -i "s|^CONF_FILE:STRING=.*|CONF_FILE:STRING=$WORK/edit/cfg/prj.conf|" "$WORK/edit/b/CMakeCache.txt"
expect "11. a cache EDIT (ccmake, an editor) after discovery stays a choice" \
    "CONF=[prj.conf] DTC=[]" "$(recfg edit)"

mk stop ""
touch "$WORK/stop/cfg/STOP"
expect "10. (the first configure stops after discovery)" "stopped between" "$(cfg stop)"
rm "$WORK/stop/cfg/STOP"
echo "CONFIG_B=y" > "$WORK/stop/cfg/boards/b.conf"
expect "10. the next configure still rediscovers" "CONF=[prj.conf;boards/b.conf]" "$(recfg stop)"

if [ "$fail" -ne 0 ]; then echo "cmake-zephyr-discovered-config: FAILED"; exit 1; fi
echo "cmake-zephyr-discovered-config: OK"
