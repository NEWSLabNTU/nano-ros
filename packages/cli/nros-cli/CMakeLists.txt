# packages/codegen/packages/nros-cli/CMakeLists.txt
#
# Builds the canonical `nros` binary via Corrosion (Cargo integration).
# `nros codegen …` (Phase 195.A — merged in the former standalone
# `nros-codegen`) is invoked by NanoRosGenerateInterfaces.cmake to generate
# C/C++ bindings from .msg/.srv/.action files.
#
# Included from the top-level CMakeLists.txt via add_subdirectory(). Uses the
# codegen workspace Cargo.toml (one level up). The Corrosion target is the bin
# name `nros`, reached at codegen time via `$<TARGET_FILE:nros>`.

corrosion_import_crate(
    MANIFEST_PATH "${CMAKE_CURRENT_SOURCE_DIR}/../Cargo.toml"
    CRATES        nros-cli
    LOCKED
)

# nano-ros Phase 140 — no install layout; the superproject consumes this
# submodule via add_subdirectory() and reaches the Corrosion-built `nros`
# target by name. No `cmake --install` step.
