# NanoRosArtifactLayout.cmake — the names of the per-build header directories
# (issue 1852, RFC-0103 D7).
#
# The `nros-c` / `nros-cpp` build scripts write `<dir>/nros/<header>` where
# `<dir>` is `nros-c-generated` / `nros-cpp-generated`. The ONE definition is
# `nros_build_paths::artifact::{C,CPP}_GENERATED_DIR` (Rust). A cargo target
# dir is asked through `nros_locate_artifact()`; this module serves the cmake
# sites that join the same names under a DIFFERENT root — the per-image mirror
# `_nros_generated_header_dir()` and the Zephyr header root — which `nros
# locate artifact` cannot answer because it only knows cargo's target dir.
#
# The names are READ from `artifact.rs`, not restated, so renaming the Rust
# constant moves every cmake reader with it. `check-artifact-path-literals`
# refuses a cmake/just/shell/Rust line that spells either name outside this
# module and its allowlist.
include_guard(GLOBAL)

set(_NROS_ARTIFACT_RS
    "${CMAKE_CURRENT_LIST_DIR}/../packages/tooling/nros-build-paths/src/artifact.rs"
    CACHE INTERNAL "issue 1852 — the Rust definition of the generated-header dir names")

# nros_generated_dir(<out> <c|cpp>)
#   The directory NAME (`nros-c-generated` / `nros-cpp-generated`).
function(nros_generated_dir _out _lang)
    if(_lang MATCHES "^c$")
        set(_const C_GENERATED_DIR)
    elseif(_lang MATCHES "^cpp$")
        set(_const CPP_GENERATED_DIR)
    else()
        message(FATAL_ERROR "nros_generated_dir: language must be `c` or `cpp`, got `${_lang}`")
    endif()
    if(NOT EXISTS "${_NROS_ARTIFACT_RS}")
        message(FATAL_ERROR
            "nros_generated_dir: ${_NROS_ARTIFACT_RS} is missing — it is the one "
            "definition of the generated-header directory names (issue 1852).")
    endif()
    file(STRINGS "${_NROS_ARTIFACT_RS}" _line
        REGEX "^pub const ${_const}: &str = \"[^\"]+\";")
    if(NOT _line MATCHES "\"([^\"]+)\"")
        message(FATAL_ERROR
            "nros_generated_dir: no `pub const ${_const}: &str = \"…\";` in "
            "${_NROS_ARTIFACT_RS} — update this reader with the constant.")
    endif()
    set(${_out} "${CMAKE_MATCH_1}" PARENT_SCOPE)
endfunction()

# nros_generated_header(<out> <root> <c|cpp> <file>)
#   `<root>/<dir>/nros/<file>` — a per-build header under a mirror or include
#   root, e.g. `nros_generated_header(_h "${_gen_dir}" c nros_config_generated.h)`.
function(nros_generated_header _out _root _lang _file)
    nros_generated_dir(_dir "${_lang}")
    set(${_out} "${_root}/${_dir}/nros/${_file}" PARENT_SCOPE)
endfunction()
