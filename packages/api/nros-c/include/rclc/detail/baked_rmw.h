/* SPDX-License-Identifier: Apache-2.0 */
/**
 * @file baked_rmw.h
 * @ingroup grp_init
 * @brief The C surface's consumer of the BAKED RMW rung (`NROS_ENTRY_RMW`).
 *
 * ## Why this file exists
 *
 * `nano_ros_entry()` bakes `NROS_ENTRY_RMW="<name>"` onto every entry target,
 * whatever its language — it is on the C compile line exactly as it is on the
 * C++ one. Until issue 1531 it had only C++ consumers
 * (`nros-cpp/include/nros/executor.hpp`, `node.hpp`), so a C image held its own
 * answer in its own preprocessor and never passed it: every C main called a
 * support init whose `rmw` is `NULL`.
 *
 * That was half of issue 1050. The same commit that added the baked rung made a
 * selector-less open with more than one registered backend a hard refusal
 * (`BackendResolution::Ambiguous`), so the C road got the new failure mode and
 * not the new capability. Issue 1530 is what that cost: a recorder
 * self-registering into every native C and C++ image made the registry
 * ambiguous, and every native C example failed `rclc_support_init` with
 * `RCL_RET_INVALID_ARGUMENT` for eighteen days.
 *
 * ## Why a macro, and why HERE
 *
 * The consumer has to be header-side. There are twenty-odd in-tree C mains plus
 * every out-of-tree C app, and rewriting call sites would be fixing the sites
 * rather than the class. A function-like macro is the C analogue of what the C++
 * headers do at their own call site, and it only expands when followed by `(`,
 * so `&rclc_support_init` is still the function's address. rclc's
 * `rclc_support_init(&support, argc, argv, &allocator)` (phase-492 W1.b) has no
 * selector argument to fill, so its macro forwards to the seam
 * `nros_c_support_init_baked`, which is that init plus the selector.
 *
 * Placement is load-bearing in two directions:
 *
 *  * it must come AFTER `<rclc/detail/nros_generated.h>`, because that header
 *    DECLARES `rclc_support_init(...)` and a function-like macro of the same
 *    name would eat the declaration. `<rclc/types.h>` includes this file last
 *    for that reason;
 *  * it must be reached by EVERY consumer, not just those including
 *    `<rclc/init.h>`. Partial coverage would be worse than none: two TUs in one
 *    image would disagree about whether the bake is passed, and the one that
 *    missed it would fail exactly the way 1530 failed. `<rclc/types.h>` is the
 *    single include every module header already goes through.
 *
 * It is NOT in `<rclc/detail/entry_config.h>`, which owns the rest of the
 * `NROS_ENTRY_*` ladder: that header is included by `app_main.h` / `main.h`
 * BEFORE any declaration exists, and its documented contract is "preprocessor
 * only — no types, no includes, no linkage".
 *
 * ## Precedence
 *
 * This is the BAKED rung of RFC-0045's precedence model A, and it stays below
 * the environment: `rclc_support_init_rmw` hands the selector to
 * `ExecutorConfig::try_resolve`, where `$NROS_RMW` still wins, alongside every
 * other field's precedence. An entry that wants to name a backend explicitly
 * calls `rclc_support_init_rmw` itself, and nothing here overrides it.
 *
 * An image with no RMW bake gets only the `rclc_support_init` macro, with a
 * NULL selector, so a single-backend registry resolves namelessly as it
 * always has.
 *
 * ## The locator and domain (phase-499 W2)
 *
 * `rclc_support_init` also forwards `NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID`
 * (`<rclc/detail/entry_config.h>`'s ladder), for the same reason as the selector: they
 * are macros in the CALLER's translation unit, which the Rust init cannot see.
 * An empty locator (the ladder's bottom rung) is passed as NULL, "absent", so
 * the backend applies its own default exactly as before. This is still the
 * baked rung: the hosted environment wins in the resolver.
 */

#ifndef NROS_BAKED_RMW_H
#define NROS_BAKED_RMW_H

#include "rclc/detail/entry_config.h"

#ifdef NROS_ENTRY_RMW

/* `argv` is cast because a program's `main` hands over `char**`, and C has no
 * implicit `char**` -> `const char* const*` conversion (C++ does). GCC 14 makes
 * `-Wincompatible-pointer-types` an error, so the riscv-none-elf 14.2 images
 * stopped compiling once phase-499 W2 had every C example pass its own argv.
 * The callee only reads it, so the cast adds const and nothing else. */
/* The nameless spellings forward the bake. `rclc_support_init_rmw` is the
 * one that takes a selector, so it is deliberately NOT wrapped — a caller
 * naming a backend has already answered the question this macro answers.
 *
 * An empty `NROS_ENTRY_RMW` is "unset", not a backend named "": the bake macro
 * expands to a string literal and an unresolved cmake variable produces `""`,
 * which `rclc_support_init_rmw` already treats as no selector (issue 1050
 * defect 3). So no `#if` on the VALUE is needed here, only on its presence. */
#define rclc_support_init(support, argc, argv, allocator)                                          \
    nros_c_support_init_baked(                                                                     \
        (support), (argc), (const char* const*)(argv), (allocator), NROS_ENTRY_RMW,                \
        (NROS_ENTRY_LOCATOR)[0] != '\0' ? (NROS_ENTRY_LOCATOR) : (const char*)0,                   \
        (uint8_t)(NROS_ENTRY_DOMAIN_ID))

#define rclc_support_init_with_locator(support, locator, domain_id)                                \
    rclc_support_init_rmw((support), (locator), (domain_id), NULL, NROS_ENTRY_RMW)

#define rclc_support_init_named(support, locator, domain_id, session_name)                         \
    rclc_support_init_rmw((support), (locator), (domain_id), (session_name), NROS_ENTRY_RMW)

#else /* !NROS_ENTRY_RMW */

#define rclc_support_init(support, argc, argv, allocator)                                          \
    nros_c_support_init_baked(                                                                     \
        (support), (argc), (const char* const*)(argv), (allocator), (const char*)0,                \
        (NROS_ENTRY_LOCATOR)[0] != '\0' ? (NROS_ENTRY_LOCATOR) : (const char*)0,                   \
        (uint8_t)(NROS_ENTRY_DOMAIN_ID))

#endif /* NROS_ENTRY_RMW */

#endif /* NROS_BAKED_RMW_H */
