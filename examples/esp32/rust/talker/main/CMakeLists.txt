# Phase 212.M.7 — IDF `main` component for the esp32 Rust talker.
#
# Registers `app_main.c` and pulls in the `nano-ros` component (which
# owns the umbrella `NanoRos::NanoRos` link target). The example's
# Rust staticlib `esp32_bsp_talker` is imported via Corrosion — the
# integration shim's `add_subdirectory(<repo-root>)` already made the
# Corrosion fns available; we reuse them here for the Entry pkg
# crate sitting one level up.

idf_component_register(
    SRCS         app_main.c
    INCLUDE_DIRS "."
    REQUIRES     nano-ros)

# Import the Entry pkg crate (Cargo.toml one dir up) and link its
# staticlib output into the main component so `rust_app_main()` is
# resolvable from `app_main.c`. Corrosion is fetched by the root
# `${_nros_root}/CMakeLists.txt` that `integrations/nano-ros/`
# transitively `add_subdirectory`-s, so `corrosion_import_crate` is
# available at this point.
if(COMMAND corrosion_import_crate)
    corrosion_import_crate(
        MANIFEST_PATH "${CMAKE_CURRENT_LIST_DIR}/../Cargo.toml"
        CRATES        esp32-bsp-talker
        CRATE_TYPES   staticlib)
    # Corrosion mangles dashes to underscores in target names; the
    # staticlib output target is `<underscored>-static`.
    if(TARGET esp32_bsp_talker-static)
        target_link_libraries(${COMPONENT_LIB}
            PRIVATE esp32_bsp_talker-static)
    endif()
endif()
