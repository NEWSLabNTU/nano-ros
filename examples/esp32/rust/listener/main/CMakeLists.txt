# Phase 212.M.7 — IDF `main` component for the esp32 Rust listener.
#
# See sibling `talker/main/CMakeLists.txt` for the wiring rationale.

idf_component_register(
    SRCS         app_main.c
    INCLUDE_DIRS "."
    REQUIRES     nano-ros)

if(COMMAND corrosion_import_crate)
    corrosion_import_crate(
        MANIFEST_PATH "${CMAKE_CURRENT_LIST_DIR}/../Cargo.toml"
        CRATES        esp32-bsp-listener
        CRATE_TYPES   staticlib)
    # Corrosion mangles dashes to underscores in target names; the
    # staticlib output target is `<underscored>-static`.
    if(TARGET esp32_bsp_listener-static)
        target_link_libraries(${COMPONENT_LIB}
            PRIVATE esp32_bsp_listener-static)
    endif()
endif()
