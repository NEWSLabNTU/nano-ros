# nano-ros — the entry locator, in ONE place (issue 0946).
#
# `NROS_ENTRY_LOCATOR` is the address an embedded image dials. It was produced
# by TWO independent per-platform ladders that never consulted each other:
# `nano_ros_entry()` in NanoRosEntry.cmake, and the three RTOS typed-entry
# carriers in NanoRosNodeRegister.cmake. NanoRosEntry.cmake said so out loud —
# "Mirrors NanoRosNodeRegister.cmake freertos branch; keep in sync" — which is a
# hand-sync instruction with no gate, the same shape phase-405 W3 found for the
# ROS edition and W6 for the zenoh tx knobs.
#
# THE INVARIANT IS "ONE PRODUCER", NOT "EQUAL LITERALS".
#
# This is why the fix is a lookup and not a constant. The two ladders DISAGREE,
# and the disagreement is not obviously a bug: they cite different networks.
# The entry lane's FreeRTOS rung dials the static-lwIP gateway 192.0.3.1
# (phase-263 C2b: the default 10.0.2.0/24 slirp never answers the guest's
# gateway ARP for that net); the node-register lane's FreeRTOS rung dials the
# slirp host 10.0.2.2 and says it "matches the qemu-arm-freertos example
# deploy". Each may be correct for the lane that reaches it, and merging them
# on a reading of the code would break whichever lane lost. Deciding needs a
# QEMU run per platform, not a grep — so this file PRESERVES both answers and
# makes the choice explicit and gated, rather than flattening it.
#
# What was actually measured, by configuring real projects (issue 0946; the
# transcript is in the commit message). Both lanes, no `-D…LOCATOR` override,
# so these are the DEFAULT rungs — which is the part no fixture row exercises,
# because every RTOS fixture row passes an explicit locator:
#
#   lane           platform / board          locator
#   entry          threadx / threadx-linux   tcp/127.0.0.1:7447
#   entry          threadx / riscv64-qemu    tcp/10.0.2.2:7447
#   entry          freertos                  tcp/192.0.3.1:7447
#   entry          nuttx                     tcp/10.0.2.2:7447
#   node-register  nuttx                     tcp/10.0.2.2:7447
#   node-register  threadx                   tcp/10.0.2.2:7553
#   node-register  freertos                  tcp/10.0.2.2:7447
#
# The two lanes agree on nuttx and disagree on threadx and freertos. That
# disagreement is CARRIED FORWARD DELIBERATELY and is not tidied here.
#
# REACHABILITY, so the next reader does not re-derive it: nothing in this tree
# reaches the node-register RTOS carriers. Every in-tree
# `nano_ros_node_register` / `nros_components_register_node` call site leaves
# DEPLOY empty (no keyword, and no `<export><nano_ros deploy=…>` tuple on a
# workspace member's package.xml), while the threadx/freertos carriers require a
# non-empty DEPLOY and the nuttx one requires `nuttx IN_LIST`. They fire for an
# OUT-OF-TREE consumer package carrying a deploy tuple, which is what they were
# written for — verified by configuring a probe package that supplies one. So
# their values are live API, not dead code, and are preserved exactly.
#
# CACHE INTERNAL, not plain `set()` — a cmake function body executes in its
# CALLER's scope, and a module `include()`d inside a function frame loses its
# normal variables when that frame pops (the `_NROS_ENTRY_DIR` pitfall in
# CLAUDE.md; it broke every FreeRTOS workspace member in 287-W6, and it bit
# NanoRosRosEdition.cmake in phase-405 W3 exactly this way — a plain `set()`
# there made the list visible from some call sites and empty from others).
# `nano_ros_node_register()` includes this file from inside a function, so this
# is not a hypothetical here.

# --- THE LOCATOR LITERALS. One file, and the only file in the tree permitted to
# --- spell one for `NROS_ENTRY_LOCATOR`. `check-entry-locator-ssot` enforces it.

# The `nano_ros_entry()` lane (LAUNCH/workspace Entry pkgs and every standalone
# `nano_ros_add_executable` example). Its board rung comes FIRST: threadx-linux
# is a host simulation whose nsos `connect()` reaches the host loopback with no
# veth bridge and no root.
set(NANO_ROS_LOCATOR_ENTRY_THREADX_LINUX "tcp/127.0.0.1:7447"
    CACHE INTERNAL "nano-ros: entry-lane locator default, threadx-linux host sim")
# Static lwIP net 192.0.3.0/24 — the gateway IS the slirp host (phase-263 C2b;
# the default 10.0.2.0/24 slirp never answers the guest's gateway ARP for
# 192.0.3.1).
set(NANO_ROS_LOCATOR_ENTRY_FREERTOS "tcp/192.0.3.1:7447"
    CACHE INTERNAL "nano-ros: entry-lane locator default, freertos static lwIP")
# Every other embedded board: QEMU slirp routes the guest to the host at
# 10.0.2.2.
set(NANO_ROS_LOCATOR_ENTRY_DEFAULT "tcp/10.0.2.2:7447"
    CACHE INTERNAL "nano-ros: entry-lane locator default, QEMU slirp host")

# The `nano_ros_node_register()` RTOS typed-entry carriers. Port 7447 serves
# manual `zenohd` runs; 7553 matches the qemu-riscv64-threadx fixture port.
set(NANO_ROS_LOCATOR_NODEREG_NUTTX "tcp/10.0.2.2:7447"
    CACHE INTERNAL "nano-ros: node-register carrier locator default, nuttx")
set(NANO_ROS_LOCATOR_NODEREG_THREADX "tcp/10.0.2.2:7553"
    CACHE INTERNAL "nano-ros: node-register carrier locator default, threadx")
set(NANO_ROS_LOCATOR_NODEREG_FREERTOS "tcp/10.0.2.2:7447"
    CACHE INTERNAL "nano-ros: node-register carrier locator default, freertos")

# Resolve the connect locator for one call site.
#
#   _nros_resolve_entry_locator(<lane> <platform> <board> out_var)
#
# `lane` is `entry` or `node-register` — the two have different override
# variables and different defaults, which is the whole reason this takes a lane
# instead of just a platform.
#
# Precedence, entry lane:
#   `NROS_ENTRY_LOCATOR` (the `-D` cache override every RTOS fixture row passes)
#   > board threadx-linux > platform freertos > the QEMU-slirp default.
#
# Precedence, node-register lane:
#   `NROS_{NUTTX,THREADX,FREERTOS}_LOCATOR` > that platform's default.
#
# NOTE the platform comparison is against the RAW `NANO_ROS_PLATFORM`, NOT a
# normalized one. `nano_ros_entry()` normalizes `freertos_armcm3` → `freertos`
# for its DEPLOY membership test but NOT for this locator decision, so a build
# passing `-DNANO_ROS_PLATFORM=freertos_armcm3` takes the DEFAULT rung rather
# than the FreeRTOS one. That is pre-existing behaviour, it is preserved
# deliberately, and normalizing here would silently move such a build from
# 10.0.2.2 to 192.0.3.1 — a different network. Left as-is rather than tidied;
# see issue 0946.
function(_nros_resolve_entry_locator lane platform board out_var)
    if(NOT NANO_ROS_LOCATOR_ENTRY_DEFAULT)
        # The scope bug this file's CACHE INTERNAL exists to prevent, reported
        # as itself rather than as a mysteriously empty locator (an image that
        # dials "" falls to backend discovery, finds no router over slirp, and
        # fails at `nros::init` with no hint that cmake is at fault).
        message(FATAL_ERROR
            "nano-ros: NANO_ROS_LOCATOR_ENTRY_DEFAULT is empty at the point "
            "_nros_resolve_entry_locator() was called. That is a scope bug in "
            "NanoRosEntryLocator.cmake, not a bad locator value.")
    endif()

    if(lane STREQUAL "entry")
        if(DEFINED NROS_ENTRY_LOCATOR)
            set(_loc "${NROS_ENTRY_LOCATOR}")
        elseif(board STREQUAL "threadx-linux")
            set(_loc "${NANO_ROS_LOCATOR_ENTRY_THREADX_LINUX}")
        elseif(platform STREQUAL "freertos")
            set(_loc "${NANO_ROS_LOCATOR_ENTRY_FREERTOS}")
        else()
            set(_loc "${NANO_ROS_LOCATOR_ENTRY_DEFAULT}")
        endif()
    elseif(lane STREQUAL "node-register")
        if(platform STREQUAL "nuttx")
            set(_loc "${NROS_NUTTX_LOCATOR}")
            if(NOT DEFINED NROS_NUTTX_LOCATOR)
                set(_loc "${NANO_ROS_LOCATOR_NODEREG_NUTTX}")
            endif()
        elseif(platform STREQUAL "threadx")
            set(_loc "${NROS_THREADX_LOCATOR}")
            if(NOT DEFINED NROS_THREADX_LOCATOR)
                set(_loc "${NANO_ROS_LOCATOR_NODEREG_THREADX}")
            endif()
        elseif(platform STREQUAL "freertos")
            set(_loc "${NROS_FREERTOS_LOCATOR}")
            if(NOT DEFINED NROS_FREERTOS_LOCATOR)
                set(_loc "${NANO_ROS_LOCATOR_NODEREG_FREERTOS}")
            endif()
        else()
            message(FATAL_ERROR
                "nano-ros: _nros_resolve_entry_locator(node-register) has no "
                "rung for platform '${platform}' (expected nuttx, threadx or "
                "freertos — the three RTOS typed-entry carriers).")
        endif()
    else()
        # Loud, never a fallback: a mistyped lane that silently returned the
        # entry default would bake a plausible-looking wrong address, and the
        # only symptom is an image that connects nowhere.
        message(FATAL_ERROR
            "nano-ros: _nros_resolve_entry_locator() got lane '${lane}' "
            "(expected 'entry' or 'node-register').")
    endif()

    set(${out_var} "${_loc}" PARENT_SCOPE)
endfunction()
