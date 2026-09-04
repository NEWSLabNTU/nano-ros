---
id: 1035
title: "zenoh-pico 1.8.0 does not compile on NuttX: `#if ... == true` is not an
  integer constant expression where `true` is `(bool)1`"
status: resolved
type: bug
area: [rmw, third-party, embedded]
related: [1021, phase-415, 0910]
---

## What

Every NuttX image that links zenoh fails to build against zenoh-pico 1.8.0:

```
nros-nuttx-export-arm/include/stdbool.h:79:25:
  error: missing binary operator before token "1"
```

`include/zenoh-pico/session/keyexpr_match_template.h:132`:

```c
#if _ZP_KE_MATCH_TEMPLATE_INTERSECTS == true
```

`true` is an integer constant expression only where `<stdbool.h>` defines it as
`1`. NuttX's exported header does not:

```c
#define true  (bool)1      /* nros-nuttx-export-arm/include/stdbool.h:79 */
#define false (bool)0
```

so the line expands to `#if ... == (bool)1` and the preprocessor stops. The cast
is legal C in an expression and illegal in a `#if`, where only integer constants
and defined() may appear.

## Scope: unconditional on NuttX, 1.8.0-only

* `keyexpr_match_template.h` **does not exist at 1.7.2** — this arrived with the
  1.8.0 bump (phase-415).
* `src/session/keyexpr.c:566,568` includes it **twice**, once per match mode,
  with no feature guard. There is no configuration that avoids it.

Native and Zephyr are unaffected: their `<stdbool.h>` uses the `1`/`0` spelling,
so the comparison is well-formed there.

## How it got in

**Phase-415 verified native and Zephyr and never built NuttX.** The 1.8.0 pin
was reviewed, gated and merged (PR #299) on that evidence. This was found
afterwards, while rebuilding NuttX fixtures for an unrelated measurement — the
build failed for a reason that had nothing to do with the change under test.

The near-identical sibling is [#1021](1021-zenoh-pico-1-8-0-matching-off-build-break.md):
1.8.0 also fails to compile with `Z_FEATURE_MATCHING=0`, which is Zephyr's
configuration. **Two 1.8.0 build breaks, both invisible in a glibc/Linux default
build, both found only by compiling the actual target.** That is the pattern
worth taking from this: a version bump's verification must cover every platform
the tree ships, not the two that are convenient.

## Fix

Carried on the patch line as `92c01732`: compare against `1`.

```c
#if _ZP_KE_MATCH_TEMPLATE_INTERSECTS == 1
```

The callers keep defining the macro as `true` / `false` (`keyexpr.c:565,567`),
which is fine — only the preprocessor comparison needed an integer. `== 1` is
also correct on a conforming `<stdbool.h>`, so this is not a NuttX-only
workaround and does not fork behaviour by platform.

## Open

1. **Report upstream**, with #1021. Both are 1.8.0 regressions that appear only
   outside a default hosted build, and an embedder hitting either gets a
   compiler error pointing at a system header rather than at zenoh-pico.
2. **Sweep for the class rather than the site.** `#if` with `true`/`false` is
   the general hazard; today there is exactly one:

   ```bash
   grep -rnE '^\s*#\s*(if|elif)\b.*\b(true|false)\b' include/zenoh-pico src
   ```

   Worth re-running on every future bump — it is cheap and it is the only thing
   that would have caught this before the merge.
3. **Does the NuttX lane run anywhere that gates?** If a NuttX build had been on
   a merge-gating lane, PR #299 could not have landed broken. It is not, and
   that is the reason this reached main rather than a review lapse.

## Verified on main, 2026-09-04

Confirmed rather than assumed from the merge. `main` at `fa7d09073`, zenoh-pico
pinned at `c5853157`:

* `just nuttx build-fixtures-arm` — clean, all twelve arm rows.
* The six NuttX C/C++ `rtos_e2e` cells — **6 / 6 PASS**, 134.6 s wall.
* `test_rtos_action_e2e` NuttX C++, `--retries 0`, 100 runs — **100 / 100**.

**The half worth recording is the observability one.** While this stood, the
nightly `nuttx` platform cell was red for it on every run in the window (CI run
`33847619657` carries the same `stdbool.h:79:25` error), so the lane had no
signal capacity at all — anything else landing in NuttX looked exactly like
yesterday's failure. That is the shape issue 0876 rode in on, and it is why this
was worth more than "a build break": it took the only observer offline for
issue 0870, whose entire remaining plan is to catch a failing run in a sweep.

A duplicate of this issue was filed the same day (id 1047, from a session that
had not found this one) and retired without reaching `main`.
