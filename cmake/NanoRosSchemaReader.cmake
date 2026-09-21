# NanoRosSchemaReader.cmake -- what a schema refusal must be able to cite.
#
# Issue 1389. A versioned fragment (`entity_inventory.cmake`,
# `message_bounds.cmake`) states a schema number; a cmake module holds the
# number it understands; when they differ the reader REFUSES rather than
# reading fields that may have moved. That much was right. What the refusal
# could not do was say WHICH READER ANSWERED, and on the one failure it was
# built for, the advice it gave instead was wrong.
#
# WHAT HAPPENED
#
# tier 2 died in configure of `build-cortex-m-c-talker-zenoh` against the
# persistent workspace under `$NROS_STORE/workspaces/zephyr/3.7/`:
#
#     ... states entity-inventory schema version 6; this reader understands 3.
#       Rebuild the `nros` CLI so the producer and the reader come from one tree
#
# Producer and reader both read 6 in the checkout that drove the configure, and
# no `3` existed anywhere in it. Rebuilding the CLI would have moved neither
# number. The `3` came from a SECOND nano-ros checkout: the persistent
# workspace's west MANIFEST PROJECT was still the runner's provisioning tree,
# so Zephyr's module lane loaded ITS `NanoRosEntityInventory.cmake` as well.
#
# MEASURED, because it is the part that reads like it cannot happen:
#
#   * `include_guard(GLOBAL)` keys on the RESOLVED PATH. Two copies of one
#     module at two paths are two guards, so BOTH bodies run.
#   * `set(<x> <v> CACHE INTERNAL ...)` implies FORCE, so the second copy
#     overwrites the first -- and redefines the first's `function()`s besides.
#     The reader that answers is whichever copy ran LAST, which on a west build
#     is the manifest project's, i.e. the older one.
#   * That also disposes of the tempting explanation. The constant is `CACHE
#     INTERNAL` and a persistent build dir keeps its `CMakeCache.txt`, so a
#     STALE CACHED READER looks like the obvious cause -- and it is not one:
#     because INTERNAL implies FORCE, a module bump is picked up on the very
#     next configure of the same build dir. Bumping the literal from 3 to 6 and
#     re-configuring a reused build dir answers 6, cache entry and all.
#
# WHAT THIS MODULE IS FOR
#
# A refusal states evidence it actually has. It has three facts and used only
# two of them:
#
#   1. the artifact's version, and the artifact's path -- it had these;
#   2. the reader's version -- it had this;
#   3. the FILE the reader's version came from -- it did not, and that is the
#      one that separates "the producer is stale" from "you have two nano-ros
#      trees in one configure".
#
# And the DIRECTION of the mismatch is a diagnosis on its own, which the old
# text ignored by giving one piece of advice for both:
#
#   artifact NEWER than reader  =>  the READER is behind. A newer CLI cannot
#       have written an older fragment, so rebuilding it changes nothing. Ask
#       which module file answered.
#   artifact OLDER than reader  =>  the ARTIFACT is stale, which IS the case
#       the old advice was written for.
#
# `<VAR>_FROM` beside every `<VAR>` is how fact 3 survives. It is `CACHE
# INTERNAL` for the reason its partner is (see `_NROS_ENTRY_DIR`): these files
# are reachable from inside a function frame, and with `include_guard` a
# file-scope plain `set()` that lands in such a frame is gone when the frame
# pops and never comes back. Gated by `check-schema-reader-provenance`.
#
# THIS file sets no file-scope variable of its own, only a function, so it is
# immune to that hazard by construction -- functions are global and survive the
# frame that included them. Keep it that way: a constant added here would need
# the `CACHE INTERNAL` treatment its callers' constants get.
include_guard(GLOBAL)

# nros_schema_mismatch_diagnosis(<out_var>
#     SUPPORTED_VAR <name>        # reader constant; <name>_FROM names its file
#     ARTIFACT <path>             # the fragment that stated a version
#     ARTIFACT_VERSION <n>        # what it stated ("" if it stated none)
#     REGENERATE <text>)          # how to re-emit the artifact
#
# Returns the provenance-and-direction half of a refusal. The caller supplies
# the first line and chooses the severity -- the message-bounds join warns
# where the entity reader is fatal, deliberately (its producer runs LATER in
# the same configure, so a fatal there would wedge the build dir).
function(nros_schema_mismatch_diagnosis _out)
    cmake_parse_arguments(_a "" "SUPPORTED_VAR;ARTIFACT;ARTIFACT_VERSION;REGENERATE" "" ${ARGN})

    set(_supported "${${_a_SUPPORTED_VAR}}")
    set(_from "${${_a_SUPPORTED_VAR}_FROM}")

    if(_from STREQUAL "")
        # No provenance recorded. Say so rather than inventing one -- a refusal
        # that guesses its own source is the defect this module exists for.
        set(_where
            "  This reader's number came from an unrecorded file: "
            "`${_a_SUPPORTED_VAR}_FROM`\n"
            "  is not set, so the module that answered cannot be named -- which\n"
            "  `check-schema-reader-provenance` exists to prevent.\n")
        set(_which_file "the module file that answered")
    else()
        set(_where
            "  This reader's number comes from:\n"
            "    ${_from}\n")
        set(_which_file "the file named above")
    endif()

    # `NOT DEFINED` FIRST, and the numeric shape checked before comparing.
    # `if(<unset-var> STREQUAL "")` is FALSE, not true: cmake dereferences the
    # left operand only when it names a DEFINED variable and otherwise compares
    # the literal string `_a_ARTIFACT_VERSION`. Measured -- the first draft fell
    # through to "the artifact is OLDER" for a fragment that stated no version
    # at all, which is a live path: the message-bounds join reaches this with an
    # undefined version on purpose (`NOT DEFINED ... OR NOT ... EQUAL ...`).
    if(NOT DEFINED _a_ARTIFACT_VERSION OR
       NOT "${_a_ARTIFACT_VERSION}" MATCHES "^[0-9]+$")
        set(_why
            "  The artifact states no readable version, so the DIRECTION of the\n"
            "  mismatch cannot be read and either side may be the stale one.\n"
            "  Check ${_which_file} first, then: ${_a_REGENERATE}\n")
    elseif(_a_ARTIFACT_VERSION GREATER _supported)
        # The producer is AHEAD. Rebuilding it cannot help, and saying so was
        # what sent issue 1389's triage the wrong way for three runs.
        set(_why
            "  The artifact is NEWER than this reader, so the READER is behind and\n"
            "  rebuilding the `nros` CLI would move neither number.\n"
            "  If ${_which_file} is not inside the checkout you are building,\n"
            "  TWO nano-ros trees have reached one configure and the older one\n"
            "  answered: `include_guard(GLOBAL)` keys on the resolved path, so both\n"
            "  copies run and the last one wins. For a persistent Zephyr workspace\n"
            "  that is usually its west MANIFEST PROJECT -- check it with\n"
            "  `scripts/check-zephyr-workspace-checkout.sh` and repair it with\n"
            "  `scripts/zephyr/unbind-manifest-project.sh --from-config <workspace>`.\n"
            "  Otherwise this checkout's module file is genuinely older than its CLI.\n")
    else()
        set(_why
            "  The artifact is OLDER than this reader, so the artifact is stale.\n"
            "  ${_a_REGENERATE}\n")
    endif()

    string(CONCAT _msg ${_where} ${_why})
    set(${_out} "${_msg}" PARENT_SCOPE)
endfunction()
