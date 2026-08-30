/* Issue 0360/0369 — the archive side of the nros-c variant stamp.
 * Issue 0904 — this file is TRACKED rather than generated into `OUT_DIR`.
 *
 * The symbol NAME is the whole payload, and it arrives as
 * `-DNROS_VARIANT_SYMBOL=nros_config_variant_<slug>`. An archive built with a
 * different feature set therefore simply does not define what a consumer's
 * header asked for, and the linker says so.
 *
 * WHY TRACKED, AND NOT WRITTEN INTO OUT_DIR (issue 0904).
 *
 * A generated TU carries its absolute `OUT_DIR` into the object, in `__FILE__`
 * and the debug-info compilation dir, which made otherwise-identical
 * `libnros_c.a` artifacts differ between two target dirs. phase-340 W6 tried to
 * mask that with `-ffile-prefix-map`; measured on the NuttX cross build, the
 * flag was not taking effect and the path was still there — with the probe
 * running and REPORTING SUPPORTED, so nothing went red.
 *
 * Compiling a tracked file removes the dependence instead of masking it: the
 * source path is this checkout's, identical in every target dir, so the object
 * is deterministic by construction rather than by a flag that has to keep
 * working silently. (It is still a per-CHECKOUT path — this buys cross-target-dir
 * identity, which is what W6 was for, not cross-machine reproducibility.)
 */

#ifndef NROS_VARIANT_SYMBOL
#error "NROS_VARIANT_SYMBOL must be defined by the build script (issue 0360)"
#endif

/* WEAK on purpose: a mixed C+C++ image links TWO nros-c builds (the C half's and
 * the one embedded in nros-cpp). With the issue-0369 size-derived suffix they
 * agree on the name, so two STRONG definitions would collide; N identical weak
 * ones merge. */
__attribute__((weak)) const unsigned char NROS_VARIANT_SYMBOL = 0;
