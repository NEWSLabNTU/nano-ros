/* RFC-0089 / phase-429 W1 — the runtime side of the CODEGEN VERSION token.
 *
 * A generated C/C++ artifact references `nros_codegen_version_v<G>`, where G is
 * the codegen version it was emitted at. This file defines one such symbol for
 * every K in the closed range [NROS_CODEGEN_VERSION_MIN, NROS_CODEGEN_VERSION],
 * which the build script passes in as `-D`s read from nros-core's
 * `codegen_version.rs` (the one source of truth for the range).
 *
 * So the accepted range IS THE SET OF DEFINED SYMBOLS. Nothing compares a
 * generated number against a runtime number at link time or at run time, so
 * there is no comparison that could itself be wrong: a generated tree the
 * runtime does not accept references a symbol nobody defined, and ld names it.
 *
 * WHY TRACKED, AND NOT WRITTEN INTO OUT_DIR (issue 0904).
 *
 * Identical to `variant_symbol.c` beside it: a generated TU carries its absolute
 * `OUT_DIR` into `__FILE__` and the debug-info compilation dir, which made
 * otherwise-identical `libnros_c.a` artifacts differ between two target dirs.
 * Compiling a TRACKED file removes the dependence instead of masking it with
 * `-ffile-prefix-map` (measured NOT taking effect on the NuttX cross build,
 * while the support probe reported it fine). Only the `-D`s vary here.
 *
 * WEAK, for the same reason `variant_symbol.c` is weak: a mixed C+C++ image
 * links TWO nros-c builds (the C half's and the one embedded in nros-cpp), and
 * both define the same range. Two STRONG definitions would collide; N identical
 * weak ones merge.
 */

#ifndef NROS_CODEGEN_VERSION
#error "NROS_CODEGEN_VERSION must be defined by the build script (RFC-0089)"
#endif
#ifndef NROS_CODEGEN_VERSION_MIN
#error "NROS_CODEGEN_VERSION_MIN must be defined by the build script (RFC-0089)"
#endif

/* The ladder below covers versions 1..32. Raising NROS_CODEGEN_VERSION needs no
 * new logic until it passes 32 — and passing 32 is LOUD rather than a silently
 * missing anchor, which would read to a user exactly like an unsupported
 * version. Extend the ladder then. */
#if NROS_CODEGEN_VERSION > 32
#error "extend the anchor ladder in codegen_version_symbols.c past 32 (RFC-0089)"
#endif

#define NROS_CODEGEN_VERSION_ANCHOR(k)                                                             \
    __attribute__((weak)) const unsigned char nros_codegen_version_v##k = 0;

/* `k` must be a literal here, not the macro argument: `##` pastes the token as
 * written, so a loop-shaped spelling is not available in the preprocessor. */
#define NROS_CODEGEN_VERSION_ACCEPTED(k)                                                           \
    (NROS_CODEGEN_VERSION_MIN <= (k) && (k) <= NROS_CODEGEN_VERSION)

#if NROS_CODEGEN_VERSION_ACCEPTED(1)
NROS_CODEGEN_VERSION_ANCHOR(1)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(2)
NROS_CODEGEN_VERSION_ANCHOR(2)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(3)
NROS_CODEGEN_VERSION_ANCHOR(3)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(4)
NROS_CODEGEN_VERSION_ANCHOR(4)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(5)
NROS_CODEGEN_VERSION_ANCHOR(5)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(6)
NROS_CODEGEN_VERSION_ANCHOR(6)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(7)
NROS_CODEGEN_VERSION_ANCHOR(7)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(8)
NROS_CODEGEN_VERSION_ANCHOR(8)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(9)
NROS_CODEGEN_VERSION_ANCHOR(9)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(10)
NROS_CODEGEN_VERSION_ANCHOR(10)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(11)
NROS_CODEGEN_VERSION_ANCHOR(11)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(12)
NROS_CODEGEN_VERSION_ANCHOR(12)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(13)
NROS_CODEGEN_VERSION_ANCHOR(13)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(14)
NROS_CODEGEN_VERSION_ANCHOR(14)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(15)
NROS_CODEGEN_VERSION_ANCHOR(15)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(16)
NROS_CODEGEN_VERSION_ANCHOR(16)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(17)
NROS_CODEGEN_VERSION_ANCHOR(17)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(18)
NROS_CODEGEN_VERSION_ANCHOR(18)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(19)
NROS_CODEGEN_VERSION_ANCHOR(19)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(20)
NROS_CODEGEN_VERSION_ANCHOR(20)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(21)
NROS_CODEGEN_VERSION_ANCHOR(21)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(22)
NROS_CODEGEN_VERSION_ANCHOR(22)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(23)
NROS_CODEGEN_VERSION_ANCHOR(23)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(24)
NROS_CODEGEN_VERSION_ANCHOR(24)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(25)
NROS_CODEGEN_VERSION_ANCHOR(25)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(26)
NROS_CODEGEN_VERSION_ANCHOR(26)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(27)
NROS_CODEGEN_VERSION_ANCHOR(27)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(28)
NROS_CODEGEN_VERSION_ANCHOR(28)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(29)
NROS_CODEGEN_VERSION_ANCHOR(29)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(30)
NROS_CODEGEN_VERSION_ANCHOR(30)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(31)
NROS_CODEGEN_VERSION_ANCHOR(31)
#endif
#if NROS_CODEGEN_VERSION_ACCEPTED(32)
NROS_CODEGEN_VERSION_ANCHOR(32)
#endif
