# NanoRosArchivePairing.cmake — assert a generated header and the archive it
# describes came from the SAME build (issues 1046 / 1050, phase-424).
#
# ---------------------------------------------------------------------------
# The class this exists to close
# ---------------------------------------------------------------------------
#
# Issue 1046: a guard asserted a module DIRECTORY, which outlives the build that
# linked it, so the guard passed on exactly the tree it existed to reject. The
# lesson is not "check a different path" — it is that a check must assert
# something that CANNOT outlive the build it reports on. A directory can. A
# file's CONTENT cannot.
#
# Issue 1050 is the same shape one layer up: a recipe consumed `libnros_cpp.a`
# without building it, so the archive on disk was whatever someone built last.
# Its "coupling is circular" note is the part this file answers — the generated
# header the module compiles against comes from the SAME cargo invocation as the
# archive, so a mismatched pair is two independent survivors of two different
# builds, and nothing checked they agree.
#
# Measured on this repo 2026-09-04, in the shared checkout, with no build
# running:
#
#     target/nros-cpp-generated/nros/nros_cpp_config_generated.h   PRESENT
#         (declaring nros_cpp_config_variant_..._rmw_zenoh_cffi_...)
#     target/release/libnros_cpp.a                                 ABSENT
#
# The header outlived its archive entirely. `NanoRosPx4Module.cmake` guarded
# that header with `IS_DIRECTORY` on its parent, which is 1046's predicate
# verbatim: it cannot tell *present* from *current*.
#
# ---------------------------------------------------------------------------
# Why a BYTE SCAN and not `nm`
# ---------------------------------------------------------------------------
#
# The variant stamp is a Rust `#[unsafe(no_mangle)] pub static` (issue 0360,
# emitted by nros-build-helpers). The SYSTEM `nm` cannot read it. Measured here
# on `target/release/libnros_cpp.a`:
#
#     nm --defined-only  | grep -c nros_cpp_config_variant   ->  0
#     <sysroot>/llvm-nm  | grep -c nros_cpp_config_variant   ->  1
#     grep -ac           (byte scan)                         ->  3
#
#     bfd plugin: LLVM gold plugin has failed to create LTO module:
#       Opaque pointers are only supported in -opaque-pointers mode
#       (Producer: 'LLVM22.1.6-rust-1.97.1-stable' Reader: 'LLVM 14.0.0')
#     nm: nros_cpp-….rcgu.o: no symbols
#
# System `nm` reports ZERO for a symbol that is demonstrably there, and it does
# not fail while doing it. A guard written on `nm` would report an absence it
# cannot observe — a NEW instance of issue 1046, which is the mistake this file
# exists to stop making. `file(STRINGS)` reads bytes and needs no binutils, no
# PATH entry, and no toolchain agreement.
#
# Cost, measured on the 25 MB archive: 0.004 s when the symbol is present
# (`LIMIT_COUNT 1` short-circuits at the first hit) and 0.243 s when it is not
# (a full scan). Both are noise at configure time, and the expensive direction
# is the one that is about to save a ~10-minute PX4 build.

include_guard(GLOBAL)

# nros_assert_archive_pairs_with_header(
#     ARCHIVE       <path to the .a the link will use>
#     HEADER        <path to the per-build generated header>
#     SYMBOL_PREFIX <nros_cpp_config_variant_ | nros_config_variant_>
#     BUILD_HINT    <the command that regenerates BOTH>
#     [LABEL        <name used in messages; defaults to the header basename>])
#
# Fails at CONFIGURE time. The alternative is the anchor firing at LINK time,
# ~1100 targets and ten minutes later, as `undefined reference to
# nros_cpp_config_variant_<the variant it wanted>` — which names the variant but
# not the mistake, and arrives long after the information is cheap to act on.
# That trade is already the stated policy of `_nros_px4_resolve_archive`; this
# extends it from "does the archive exist" to "is it the RIGHT one".
function(nros_assert_archive_pairs_with_header)
    cmake_parse_arguments(NAP "" "ARCHIVE;HEADER;SYMBOL_PREFIX;BUILD_HINT;LABEL" "" ${ARGN})

    foreach(_req ARCHIVE HEADER SYMBOL_PREFIX BUILD_HINT)
        if(NOT NAP_${_req})
            message(FATAL_ERROR
                "nros_assert_archive_pairs_with_header: ${_req} is required")
        endif()
    endforeach()
    if(NOT NAP_LABEL)
        get_filename_component(NAP_LABEL "${NAP_HEADER}" NAME)
    endif()

    # (1) The header must be a FILE.
    #
    # Not "its directory exists" — that is issue 1046's predicate, and the
    # directory demonstrably survives builds that wrote nothing into it. Not
    # `EXISTS` alone either: `EXISTS` is true for a directory, so a header path
    # that has become a directory would pass a bare existence test.
    if(NOT EXISTS "${NAP_HEADER}" OR IS_DIRECTORY "${NAP_HEADER}")
        message(FATAL_ERROR
            "nros_assert_archive_pairs_with_header: the per-build generated header is missing:\n"
            "    ${NAP_HEADER}\n"
            "Its DIRECTORY existing is not evidence it was written — a generated\n"
            "header outlives the build that produced it, and an empty generated\n"
            "dir outlives the build that emptied it (issue 1046).\n"
            "Build it:\n"
            "    ${NAP_BUILD_HINT}")
    endif()

    # (2) The header must carry a variant stamp, and we read the symbol OUT of
    #     it rather than recomputing it. Recomputing would mean a second
    #     derivation of the slug — the class CLAUDE.md names for `row_coord()`,
    #     where the second derivation left 67 rows in no lane at all. The header
    #     is the SSoT for what the consumer will reference; ask it.
    file(STRINGS "${NAP_HEADER}" _nap_decls
        REGEX "${NAP_SYMBOL_PREFIX}[A-Za-z0-9_]+")
    set(_nap_symbol "")
    foreach(_line IN LISTS _nap_decls)
        if(_line MATCHES "(${NAP_SYMBOL_PREFIX}[A-Za-z0-9_]+)")
            set(_nap_symbol "${CMAKE_MATCH_1}")
            break()
        endif()
    endforeach()

    if(NOT _nap_symbol)
        message(FATAL_ERROR
            "nros_assert_archive_pairs_with_header: ${NAP_LABEL} carries no "
            "'${NAP_SYMBOL_PREFIX}*' variant stamp:\n"
            "    ${NAP_HEADER}\n"
            "That stamp is what makes a header/archive mismatch a LINK error "
            "instead of a silent buffer-size disagreement (issue 0360). A header "
            "without one is either the checked-in stub or predates the stamp; "
            "either way the pairing below cannot be checked and the sizes cannot "
            "be trusted.\n"
            "Regenerate it:\n"
            "    ${NAP_BUILD_HINT}")
    endif()

    # (3) The archive must DEFINE that symbol. Content against content: neither
    #     side can outlive the other's build without this firing.
    if(NOT EXISTS "${NAP_ARCHIVE}")
        message(FATAL_ERROR
            "nros_assert_archive_pairs_with_header: archive not found:\n"
            "    ${NAP_ARCHIVE}\n"
            "Build it:\n    ${NAP_BUILD_HINT}")
    endif()

    file(STRINGS "${NAP_ARCHIVE}" _nap_hit REGEX "${_nap_symbol}" LIMIT_COUNT 1)
    if(NOT _nap_hit)
        message(FATAL_ERROR
            "nros_assert_archive_pairs_with_header: the generated header and the "
            "archive are from DIFFERENT builds.\n"
            "    header:  ${NAP_HEADER}\n"
            "    expects: ${_nap_symbol}\n"
            "    archive: ${NAP_ARCHIVE}\n"
            "    defines: (not that symbol)\n"
            "\n"
            "The header carries storage SIZES and the archive was compiled with "
            "others, so linking them is the issue-0268 silent-overflow class. "
            "The two are independent files that each outlive the build that "
            "wrote them (issues 1046/1050), which is why the mismatch is "
            "possible at all: nothing rebuilds one when the other moves.\n"
            "\n"
            "Rebuild BOTH from one cargo invocation — they are the same "
            "byproduct:\n"
            "    ${NAP_BUILD_HINT}\n"
            "If you overrode the archive path, the override did not move the "
            "header with it; the header path is derived from the nano-ros "
            "checkout, the archive path is not.")
    endif()

    message(STATUS "nano-ros: ${NAP_LABEL} pairs with the archive (${_nap_symbol})")
endfunction()
