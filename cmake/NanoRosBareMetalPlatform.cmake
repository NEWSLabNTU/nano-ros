# cmake/NanoRosBareMetalPlatform.cmake — issue 1512.
#
# `NROS_BOARD` -> the `nros-c` / `nros-cpp` cargo feature that selects this
# board's platform implementation.
#
# ## Why this file exists at all
#
# Every other platform answers "which `nros-platform/platform-X`?" with one
# name, so `nros_feature_set()`'s PLATFORM ladder can spell it inline. Bare
# metal cannot: a no-RTOS port IS the board (its clock is that board's timer,
# its net is that board's MAC, and there is no kernel in between to abstract
# either), so `nros-platform` carries three board-specific features where the
# RTOSes carry one apiece. See the long block in `packages/api/nros-c/
# Cargo.toml` for why the C road has to spell the board at all, when the Rust
# road does not.
#
# ## Which vocabulary the key is in
#
# The CMAKE BOARD token — the `<board>` in `cmake/board/nano-ros-board-
# <board>.cmake`, which is what `cmake/platform/nano-ros-baremetal.cmake`
# dispatches on and what its FATAL_ERROR tells a caller to set. That is NOT the
# board-descriptor `names` vocabulary: `packages/boards/nros-board-mps2-an385/
# nros-board.toml` lists `mps2-an385-baremetal`, `baremetal`, `bare-metal` and
# `rtic-mps2-an385` (the first since issue 2063), but those are the image
# vocabulary, not the overlay stem this dispatches on. One namespace per file;
# this one is CMake's.
#
# ## Why an authored map, and what holds it honest
#
# The value is derivable in principle — the board CRATE selects it
# unconditionally (`nros-board-mps2-an385`'s `nros-platform = { …, features =
# ["platform-mps2-an385", …] }`), which is the very fact the Rust road relies
# on — but deriving it here means regex-parsing a Cargo.toml dependency line
# from CMake, and a reformat there would break a build over here. So the map is
# authored and `check-baremetal-platform-arms` checks it in BOTH directions:
# every `cmake/board/nano-ros-board-*-baremetal.cmake` has a row, every row's
# feature exists in `nros-c` AND `nros-cpp`, and `nros-platform`'s bare-metal
# feature roster is exactly the set of values used. A missing row is a configure
# FATAL_ERROR rather than a fall-through, which is the failure mode issue 1512
# found: the old ladder reached `elseif(_cross)` and asked cargo for
# `platform-baremetal`, a feature no crate has.
include_guard(GLOBAL)

# The rows. `<cmake board token>` `<nros-c/nros-cpp feature>`, in pairs.
set(_NROS_BAREMETAL_BOARD_FEATURES
    mps2-an385-baremetal platform-mps2-an385
    CACHE INTERNAL "issue 1512 — bare-metal CMAKE board token -> nros-c platform feature")

# nros_baremetal_platform_feature(<out_var> <board> <context>)
#
# Sets <out_var> in the caller's scope to the `nros-c` / `nros-cpp` platform
# feature for bare-metal board <board>. FATAL_ERROR when <board> is empty or
# unknown; <context> names the caller in the diagnostic.
function(nros_baremetal_platform_feature out_var board context)
    set(_known "")
    set(_hit "")
    set(_key "")
    foreach(_tok IN LISTS _NROS_BAREMETAL_BOARD_FEATURES)
        if(_key STREQUAL "")
            set(_key "${_tok}")
            list(APPEND _known "${_tok}")
        else()
            if(_key STREQUAL "${board}")
                set(_hit "${_tok}")
            endif()
            set(_key "")
        endif()
    endforeach()
    string(REPLACE ";" ", " _known_flat "${_known}")
    if(board STREQUAL "")
        message(FATAL_ERROR
            "${context}: a bare-metal build must set NROS_BOARD before the "
            "runtime is imported. Bare metal is the one platform with no single "
            "`nros-platform/platform-*` feature — the board IS the port — so "
            "the C/C++ staticlib cannot be configured without knowing which "
            "board it is for. Known: ${_known_flat}.")
    endif()
    if(_hit STREQUAL "")
        message(FATAL_ERROR
            "${context}: no bare-metal platform feature for NROS_BOARD "
            "'${board}' (known: ${_known_flat}). Add the row to "
            "cmake/NanoRosBareMetalPlatform.cmake together with the matching "
            "`platform-*` arm on nros-c and nros-cpp; "
            "`just check baremetal-platform-arms` checks the three agree.")
    endif()
    set(${out_var} "${_hit}" PARENT_SCOPE)
endfunction()

# ---------------------------------------------------------------------------
# phase-553 W4 (RFC-0130 D2) — the board crate a C-rooted bare-metal image's
# staticlib carries.
#
# On a board with no RTOS the startup is Rust: cortex-m-rt owns `Reset`, the
# board crate owns clock/network bring-up and the C-ABI start
# (`nros_board_<board>_start(app)`), and the PAC supplies the vector table.
# In a C-rooted image CMake owns the final link, so all three must ride in the
# image's one Rust staticlib. The rows say which crate, with which features,
# which `#[no_mangle]` start to anchor against staticlib DCE, and which
# packages publish the linker scripts the C link needs.
#
# Rows: `<cmake board token>|<crate>|<crate dir, repo-relative>|<features,…>|
#        <start path>|<link-script packages,…>`
#
# `c-libc` is what makes newlib the image's one libc and heap owner (phase-553
# W1). Default features are off, and the backend feature is chosen by the
# image's RMW: when the board crate declares `rmw-<rmw>`, it is on. On
# mps2-an385 that is what installs the link's RX interrupt as the session's
# wake (`set_caller_rx_wakes_park`); without it the session states no
# readiness form and is refused at open. It does not register the backend:
# the image crate bundles it and `nros_app_register_backends` registers it.
#
# The link feature is the IP one (`ethernet`) unless the image's transport is a
# byte stream: then `NROS_LINK_IP` is "0" (the row `transport` implies, in the
# image env) and the board takes `serial` instead — the swap the cargo road
# makes in `image_link_features`, from the same fact.
set(_NROS_BAREMETAL_BOARD_CRATES
    "mps2-an385-baremetal|nros-board-mps2-an385|packages/boards/nros-board-mps2-an385|ethernet,c-libc|nros_board_mps2_an385::c_start::nros_board_mps2_an385_start|cortex-m-rt,nros-board-mps2-an385,mps2-an385-pac"
    CACHE INTERNAL "phase-553 W4 — bare-metal CMAKE board token -> image board crate")

# nros_baremetal_board_crate(<prefix> <board> <repo> [<rmw>])
#
# Sets, in the caller's scope:
#   <prefix>_DEP_LINE      the image crate's `[dependencies]` line
#   <prefix>_ANCHOR        Rust that keeps the board's C-ABI start in the archive
#   <prefix>_LINK_PACKAGES the packages whose `$OUT_DIR` holds linker scripts
# FATAL_ERROR for a bare-metal board with no row.
function(nros_baremetal_board_crate prefix board repo)
    set(_rmw "${ARGV3}")
    set(_hit "")
    set(_known "")
    foreach(_row IN LISTS _NROS_BAREMETAL_BOARD_CRATES)
        string(REPLACE "|" ";" _cols "${_row}")
        list(GET _cols 0 _key)
        list(APPEND _known "${_key}")
        if(_key STREQUAL "${board}")
            set(_hit "${_cols}")
        endif()
    endforeach()
    if(_hit STREQUAL "")
        string(REPLACE ";" ", " _known_flat "${_known}")
        message(FATAL_ERROR
            "nano-ros: bare-metal board '${board}' has no board crate for a C or C++ "
            "image (known: ${_known_flat}). Add its row to "
            "cmake/NanoRosBareMetalPlatform.cmake.")
    endif()
    list(GET _hit 1 _crate)
    list(GET _hit 2 _dir)
    list(GET _hit 3 _feats)
    list(GET _hit 4 _start)
    list(GET _hit 5 _links)
    string(REPLACE "," ";" _feat_list "${_feats}")
    include("${CMAKE_CURRENT_FUNCTION_LIST_DIR}/NanoRosImageEnv.cmake")
    nros_image_env_value(_link_ip NROS_LINK_IP)
    if(_link_ip STREQUAL "0" AND "ethernet" IN_LIST _feat_list)
        list(REMOVE_ITEM _feat_list ethernet)
        list(APPEND _feat_list serial)
    endif()
    if(NOT "${_rmw}" STREQUAL "" AND NOT _rmw STREQUAL "none")
        file(STRINGS "${repo}/${_dir}/Cargo.toml" _rmw_rows REGEX "^rmw-${_rmw}[ \t]*=")
        if(_rmw_rows)
            list(APPEND _feat_list "rmw-${_rmw}")
        endif()
    endif()
    set(_feat_toml "")
    foreach(_f IN LISTS _feat_list)
        string(APPEND _feat_toml "\"${_f}\", ")
    endforeach()
    get_filename_component(_abs "${repo}/${_dir}" ABSOLUTE)
    set(${prefix}_DEP_LINE
        "${_crate} = { path = \"${_abs}\", default-features = false, features = [${_feat_toml}] }"
        PARENT_SCOPE)
    set(${prefix}_ANCHOR
"// The board's C-ABI start, anchored so staticlib DCE keeps it: the board
// overlay's `main` calls it (phase-553 W4).
#[used]
static _KEEP_BOARD_START: unsafe extern \"C\" fn(Option<unsafe extern \"C\" fn() -> i32>) -> ! =
    ::${_start};
" PARENT_SCOPE)
    string(REPLACE "," ";" _link_list "${_links}")
    set(${prefix}_LINK_PACKAGES "${_link_list}" PARENT_SCOPE)
endfunction()
