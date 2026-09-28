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
 * answer in its own preprocessor and never passed it: every C main calls the
 * three-argument `nros_support_init`, whose `rmw` is `NULL`.
 *
 * That was half of issue 1050. The same commit that added the baked rung made a
 * selector-less open with more than one registered backend a hard refusal
 * (`BackendResolution::Ambiguous`), so the C road got the new failure mode and
 * not the new capability. Issue 1530 is what that cost: a recorder
 * self-registering into every native C and C++ image made the registry
 * ambiguous, and every native C example failed `nros_support_init` with
 * `NROS_RET_INVALID_ARGUMENT` for eighteen days.
 *
 * ## Why a macro, and why HERE
 *
 * The consumer has to be header-side. There are twenty-odd in-tree C mains plus
 * every out-of-tree C app, and rewriting call sites would be fixing the sites
 * rather than the class. A function-like macro is the C analogue of what the C++
 * headers do at their own call site, and it only expands when followed by `(`,
 * so `&nros_support_init` is still the function's address.
 *
 * Placement is load-bearing in two directions:
 *
 *  * it must come AFTER `<nros/nros_generated.h>`, because that header
 *    DECLARES `nros_support_init(...)` and a function-like macro of the same
 *    name would eat the declaration. `<nros/types.h>` includes this file last
 *    for that reason;
 *  * it must be reached by EVERY consumer, not just those including
 *    `<nros/init.h>`. Partial coverage would be worse than none: two TUs in one
 *    image would disagree about whether the bake is passed, and the one that
 *    missed it would fail exactly the way 1530 failed. `<nros/types.h>` is the
 *    single include every module header already goes through.
 *
 * It is NOT in `<nros/entry_config.h>`, which owns the rest of the
 * `NROS_ENTRY_*` ladder: that header is included by `app_main.h` / `main.h`
 * BEFORE any declaration exists, and its documented contract is "preprocessor
 * only — no types, no includes, no linkage".
 *
 * ## Precedence
 *
 * This is the BAKED rung of RFC-0045's precedence model A, and it stays below
 * the environment: `nros_support_init_rmw` hands the selector to
 * `ExecutorConfig::try_resolve`, where `$NROS_RMW` still wins, alongside every
 * other field's precedence. An entry that wants to name a backend explicitly
 * calls `nros_support_init_rmw` itself, and nothing here overrides it.
 *
 * An image with no bake is unchanged: with `NROS_ENTRY_RMW` undefined these
 * macros do not exist, and a single-backend registry resolves namelessly as it
 * always has.
 */

#ifndef NROS_BAKED_RMW_H
#define NROS_BAKED_RMW_H

#ifdef NROS_ENTRY_RMW

/* The two nameless spellings forward the bake. `nros_support_init_rmw` is the
 * one that takes a selector, so it is deliberately NOT wrapped — a caller
 * naming a backend has already answered the question this macro answers.
 *
 * An empty `NROS_ENTRY_RMW` is "unset", not a backend named "": the bake macro
 * expands to a string literal and an unresolved cmake variable produces `""`,
 * which `nros_support_init_rmw` already treats as no selector (issue 1050
 * defect 3). So no `#if` on the VALUE is needed here, only on its presence. */
#define nros_support_init(support, locator, domain_id)                                             \
    nros_support_init_rmw((support), (locator), (domain_id), NULL, NROS_ENTRY_RMW)

#define nros_support_init_named(support, locator, domain_id, session_name)                         \
    nros_support_init_rmw((support), (locator), (domain_id), (session_name), NROS_ENTRY_RMW)

#endif /* NROS_ENTRY_RMW */

#endif /* NROS_BAKED_RMW_H */
