# nano-ros — the ONE place that writes a component target's language.
#
# issue 1523. `NROS_COMPONENT_LANG` is a target property two verbs write and
# three comparisons read, and it had TWO SPELLINGS:
#
#   * `nano_ros_auto_add_library()` wrote the canonical lowercase `c` / `cpp`
#     (`nros_lang::Language::as_str`, what `nros codegen source-language`
#     answers, what `nano_ros_add_node` forwards as `LANGUAGE ${_lang}`, and
#     what the `"lang"` field of the emitted metadata row carries);
#   * `nano_ros_node_register()` wrote its own UPPERCASE internal vocabulary
#     (`C` / `CPP` / `RUST`, ~30 comparisons inside that one file).
#
# `STREQUAL` is case-sensitive, so all three readers — which tested `"C"` —
# were permanently FALSE for every `nano_ros_auto_add_library` target:
# `LINKER_LANGUAGE C` was never set, the `NOT … STREQUAL "C"` umbrella branch
# was always taken, and `nros_components_register_node`'s reader was dead code
# for one of its two producers. It read as correct because the two wrong
# branches cancelled into the link line issue 0425 wants.
#
# The canonical spelling is the LOWERCASE one, because it is the serde contract
# — anything else makes the property disagree with the metadata row emitted
# beside it. So `nano_ros_node_register` lower-cases at the BOUNDARY where the
# value leaves its uppercase vocabulary, which is the mirror of the `TOUPPER`
# that admits `nros_language_of_sources()`'s answer into it (phase-469 S3), and
# NOT a `string(TOUPPER)` at each reader — that would leave two vocabularies in
# the tree and one more place for the next one to disagree.
#
# This module exists so the write has ONE spelling that REFUSES the other one,
# rather than three `set_target_properties` calls that each silently accept
# whatever they are handed. `_nros_set_component_lang` is cheap, it runs at
# configure time on every component, and a rejected value names the target.
#
# Gate: `check-component-lang-vocabulary` — no site outside this module may
# write `NROS_COMPONENT_LANG`, and every `STREQUAL` against a variable bound
# from it must compare a canonical lowercase literal.
include_guard(GLOBAL)

# ---------------------------------------------------------------------------
# _nros_set_component_lang(<target> <lang>)
#
# Record the component language on <target>, refusing anything outside the
# canonical lowercase vocabulary. A caller holding the uppercase form converts
# BEFORE calling — the conversion belongs to the vocabulary that has it, not to
# this function, which would otherwise quietly accept both spellings again and
# put us back where issue 1523 started.
# ---------------------------------------------------------------------------
function(_nros_set_component_lang target lang)
    # The canonical vocabulary. `rust` never reaches `nano_ros_auto_add_library`
    # (that verb compiles C-family SOURCES) but does reach
    # `nano_ros_node_register` and its conventional-name wrapper, so all three
    # spellings live here.
    #
    # Spelled INSIDE the function, not as a file-scope variable: this module is
    # `include()`d from files that are themselves reachable inside a function
    # frame, and a normal variable set by an `include()` in a function frame is
    # gone when the frame pops (the `_NROS_ENTRY_DIR` pitfall, 287-W6) — while
    # `include_guard(GLOBAL)` means the second include would never re-set it.
    # An empty list would make `IN_LIST` reject every value, i.e. break every
    # configure rather than fail open, but "loud in the wrong place" is still
    # the wrong place.
    set(_values c cpp rust)
    if(NOT TARGET ${target})
        message(FATAL_ERROR
            "_nros_set_component_lang(${target}): no such target.")
    endif()
    if(NOT lang IN_LIST _values)
        message(FATAL_ERROR
            "_nros_set_component_lang(${target}): language '${lang}' is not a "
            "canonical NROS_COMPONENT_LANG value. Expected one of "
            "${_values} — LOWERCASE, the "
            "`Language::as_str` spelling the readers compare and the metadata "
            "row carries (issue 1523). A caller whose own vocabulary is "
            "uppercase converts at this boundary, e.g. "
            "`string(TOLOWER \"\${_nrc_lang}\" _nrc_lang_lc)`.")
    endif()
    set_property(TARGET ${target} PROPERTY NROS_COMPONENT_LANG "${lang}")
endfunction()
