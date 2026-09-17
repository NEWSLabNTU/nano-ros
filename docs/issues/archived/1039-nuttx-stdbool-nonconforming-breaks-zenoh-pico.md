---
id: 1039
title: "NuttX's `stdbool.h` defines `true` as `(bool)1`, which is not an integer
  constant — every NuttX build breaks on zenoh-pico's new keyexpr template"
status: resolved
type: bug
area: nuttx, rmw-zenoh, build
severity: high
related: [issue-1007, issue-1035, phase-355]
found: 2026-09-04
---

## Measured

Same tree, same commands, only the `zenoh-pico` pin differs:

| pin | `just nuttx build-fixtures-arm` |
| --- | --- |
| `fa7ad0f5b3f6` | **EXIT=0** (2026-09-04 11:04, leaves relinked) |
| `0101b80d5468` (what main records now) | **EXIT=2**, 12 errors |

The workspace path fails identically —
`nros build demo_bringup:nuttx --workspace examples/workspaces/c` gets past
preflight and dies in the same place:

```
error: failed to run custom build command for `zpico-sys v0.5.0`
  keyexpr_match_template.h:142:14: error: redefinition of '_z_chunk_special_includes'
  nros-nuttx-export-arm/include/stdbool.h:79:25: error: missing binary operator before token "1"
      note: in expansion of macro '_ZP_KE_MATCH_TEMPLATE_INTERSECTS'
```

`include/zenoh-pico/session/keyexpr_match_template.h` is NEW in the current pin
(upstream "Faster key expression matching (#1175)") and absent from
`fa7ad0f5b3f6`. It is not our file and not a stale artifact.

## Cause: NuttX's header is non-conforming, and the new template exposes it

`src/session/keyexpr.c` includes the template TWICE, which is its whole design:

```c
#define _ZP_KE_MATCH_TEMPLATE_INTERSECTS true
#include "zenoh-pico/session/keyexpr_match_template.h"
#define _ZP_KE_MATCH_TEMPLATE_INTERSECTS false
#include "zenoh-pico/session/keyexpr_match_template.h"
```

The macro is used both in `#if` and in `_ZP_CAT` to build distinct symbol names.
That is standard-conforming C, because C99/C11 §7.18 requires:

> `true` ... expands to the integer constant `1`, `false` ... to the integer
> constant `0`

NuttX's export header does not:

```c
/* third-party/nuttx/nuttx/nros-nuttx-export-arm/include/stdbool.h:79 */
#define true  (bool)1
#define false (bool)0
```

Two consequences, and both errors above are exactly them:

1. **`#if` breaks.** `#if _ZP_KE_MATCH_TEMPLATE_INTERSECTS` expands to `(bool)1`.
   In a preprocessor expression `bool` is an identifier, and unknown identifiers
   become `0`, giving `(0)1` — hence *"missing binary operator before token 1"*,
   reported inside `stdbool.h` because that is where the offending tokens live.
2. **Token pasting breaks.** `_ZP_CAT` applied to `(bool)1` and `(bool)0` cannot
   produce two distinct identifiers, so both expansions define
   `_z_chunk_special_includes` — hence *"redefinition ... previous definition
   with type `_Bool(const char *, ...)`"*.

A conforming `stdbool.h` makes both work, which is why every other platform
builds this file.

## Why it surfaced now, and what it says about the lane

This is the SECOND defect unmasked in this module today. The preflight fix
landed as `c003ae608` — the id 0999 was reserved for it and the file was never
written, so the commit is the only thing here anyone can look up. It stopped
`nros build` refusing NuttX at stage 3 for a build-std target; the build then
reaches the compiler and finds this. The
nuttx module of `lane=tier2` has therefore still never completed — for a new
reason, not the old one.

## Fix options, in the order I would try them

1. **Patch our zenoh-pico fork's template to use `1`/`0` rather than
   `true`/`false`.** Smallest and portable: an integer constant is what both the
   `#if` and the paste actually want, and it removes the dependency on any
   platform's `stdbool.h` being right. Goes on the patch line, not upstream
   `main` (CLAUDE.md's vendored-fork workflow).
2. **Fix NuttX's `stdbool.h`.** Correct at the root — the header is simply wrong
   per §7.18 — but it is a vendored third-party export, so it needs its own
   patch line and re-export, and it fixes only our copy.
3. **Define conforming `true`/`false` in the zpico shim before including
   zenoh-pico headers.** Cheapest to write and the most fragile: it papers over
   a broken libc header for one consumer and will not help the next one.

(1) also wants an upstream conversation: the template is fine by the standard,
but `1`/`0` costs upstream nothing and makes it robust against exactly this
class of non-conforming libc.

## Not verified

* Whether NuttX's own `stdbool.h` (upstream, not our export) has the same
  definition, or whether the export step introduces it.
* Whether `riscv` NuttX is affected — only `armv7a-nuttx-eabihf` was measured.
* Whether anything else in the tree relies on `true`/`false` in a preprocessor
  expression on NuttX; if so, this is one site of a class.

## Acceptance

* [ ] `just nuttx build-fixtures-arm` completes against main's zenoh-pico pin.
* [ ] The nuttx module of `lane=tier2` completes.
* [ ] The chosen fix is recorded with why, since option 1 and option 2 disagree
      about whose bug it is.


## 2026-09-04 — the stdbool half is FIXED; two more layers behind it

**Fixed:** `nros-zpico-build` now generates a conforming `true`/`false` header
into `OUT_DIR` and force-includes it (`-include`) ahead of every zenoh-pico TU on
NuttX, keyed on the target triple so all three exports are covered. Pulling
`<stdbool.h>` in first sets NuttX's `__INCLUDE_STDBOOL_H` guard, so the TU's own
later include is a no-op and cannot restore the casts. A `-D` cannot do this —
the header is included afterwards and wins.

Verified: `just nuttx build-fixtures-arm` goes from **12** keyexpr/stdbool errors
to **0**, and the generated header is present in all three nuttx cargo-fixture
target dirs, dated to the run.

A caution about HOW that was verified, because the obvious check is wrong:
grepping the build log for the `-include` flag returns nothing even when the fix
is applied, because the log carries errors, not compiler command lines. I read
that absence as "the fix never ran" and nearly retracted a working change. The
artifact on disk is the evidence; the log is not.

**Still failing, and it is NOT this issue:**

    error: linking with `arm-none-eabi-gcc` failed
    multiple definition of `_z_socket_get_endpoints'    (x6)

`src/system/common/platform.c:135` defines a fallback under

    #if !defined(ZENOH_WINDOWS) && !defined(ZENOH_LINUX) && !defined(ZENOH_MACOS)
        && !defined(ZENOH_BSD) && !defined(ZENOH_ZEPHYR)

and something else defines it too. Note the asymmetry this exposes:
`runner.rs`'s size probe defines **both** `ZENOH_NUTTX` and `ZENOH_LINUX`, with a
long comment explaining that dropping `ZENOH_LINUX` "would silently move every
NuttX image onto six code paths nothing here has ever exercised" — while
`packages/platform/nros-platform-nuttx/nros-platform.toml:22` defines only
`["ZENOH_GENERIC", "ZENOH_NUTTX"]`. Two authored copies of "what NuttX defines",
disagreeing. That is a strong lead and it deserves its own issue.

**HYPOTHESIS, EXPLICITLY UNTESTED.** I added `ZENOH_LINUX` to the manifest and
rebuilt; the result was byte-identical, so I first read it as refuted. It was
not tested at all: **`nros-platform.toml` is not a `rerun-if-changed` input**, so
cargo did not re-run the build script and the edit never reached the compiler.
The generated header's timestamp proves it — unchanged across that build, and
only regenerated once I touched `build.rs`. The change is reverted rather than
kept, and the hypothesis is open.

That last point is worth its own line: **a config file that is not a build input
changes nothing until something unrelated forces a rebuild**, which makes every
experiment on it silently void. Same family as the rest of this tree's staleness
defects.

## 2026-09-06 — filed late; where this stands on `main` today

This document was written on 2026-09-04 and never landed: it sat on
`fix/1016-lane-build-vs-run`, a branch whose pull request merged without it.
Meanwhile SEVEN references to "issue 1039" went in across `just/check.just`,
`scripts/check-zenoh-platform-macros.py` and
`packages/rmw/zenoh/nros-zpico-build/src/runner.rs`, all pointing at a file
that did not exist. `check-prose-issue-refs` (the gate #206 adds) is what
found it.

Two claims below have moved and are left in place rather than rewritten,
because the measurement is the record:

* The pin table reads `0101b80d5468 (what main records now)`. That was true on
  2026-09-04. `main` now records `dd071b8d3a14a72b9d2b96ff70d1cce1f4af1596`.
* The stdbool half is fixed on `main`, and NOT by the `runner.rs` workaround
  this document proposes. It was fixed in the vendored fork instead —
  `a1c741db`, "`== true` in a #if breaks every NuttX build — compare against
  1", which replaces the `true`/`false` template arguments with `1`/`0` at the
  call site in `src/session/keyexpr.c`. Verified: that commit is an ancestor of
  the pin `main` records today.

  Both routes are workarounds for the same non-conforming header; the fork-side
  one shipped, so the `runner.rs` force-include is redundant and is NOT being
  landed with this document.

The issue stays OPEN because three of the four boxes below are still unticked,
and the last of them — the root fix in NuttX's own header, or a documented
decision to carry the workaround — is the one that closes it.

## Revised acceptance

* [x] The stdbool/keyexpr class no longer breaks the NuttX zenoh-pico build.
* [x] `multiple definition of _z_socket_get_endpoints` resolved — `ZENOH_LINUX`
      is gone from `runner.rs` and gated; 19 arm images link (Resolution §4).
* [x] `nros-platform.toml` is a watched build input —
      `platform_manifests_to_watch` enumerates the platform package dirs.
* [x] The root fix in NuttX's own header, or a documented decision to carry the
      workaround instead — the decision is recorded in the Resolution below:
      carry the fork-side fix, do not add a second workaround, root fix stays
      upstream NuttX's.

## Resolution — 2026-09-18: nothing here is left to fix

Re-measured from scratch on `main` at `fa6cf68d2` rather than trusting the
2026-09-04 record: two weeks and a possible pin move are enough to invalidate
it, and this document's own history is what says so — it has already been wrong
twice about what `main` carries.

### 1. The header is still non-conforming, and it is UPSTREAM NuttX's

`third-party/nuttx/nuttx` @ `aea00d736`, `include/stdbool.h:79-80`:

```c
#  define true  (bool)1
#  define false (bool)0
```

That answers two of the "Not verified" items above, and neither answer needed a
build:

* **The export step does not introduce it — upstream's own header has it.** The
  submodule's `include/stdbool.h` and all three exports' copies are BYTE-
  IDENTICAL: `md5 8e6f58e26bf710017e1e6d33fabe1f4f` for each of
  `include/stdbool.h` and `nros-nuttx-export-{arm,arm-smp,riscv}/include/stdbool.h`.
* **riscv is affected exactly as arm is**, and for a reason that makes measuring
  it unnecessary: there is ONE header and every arch export copies it.

The nuttx fork's `nano-ros` branch tip IS the pin, and no commit on that line
touches `include/stdbool.h` (newest is upstream's SPDX migration). So the root
fix is still absent, and still belongs upstream.

### 2. The BREAKAGE is no longer reachable — and BOTH halves of the fork fix matter

zenoh-pico pin `dd071b8d3a14` — still 1.8.0 (`version.txt`), the version that
introduced the template, so the pin has NOT moved out from under this issue —
carries `a1c741db` ("`== true` in a #if breaks
every NuttX build — compare against 1") as an ancestor
(`git merge-base --is-ancestor`, rc 0), and that commit touches two files:

* `include/zenoh-pico/session/keyexpr_match_template.h:148` —
  `#if _ZP_KE_MATCH_TEMPLATE_INTERSECTS == 1`
* `src/session/keyexpr.c:573,575` — the CALL SITE defines the macro `1` / `0`

The template is still included once per value, so the design this document
describes is intact; only the spelling of the two arguments moved.

**The call-site half is load-bearing, which corrects issue 1035 and the
`README.md` entry written from it.** 1035 says the callers "keep defining the
macro as `true`/`false`, which is fine — only the preprocessor comparison
needed an integer." Measured (§3): that is false. Put the call site back to
`true`/`false` with the header's `== 1` in place and the compile fails with the
SAME 11 errors, because `#if ... == 1` still expands the `true` that arrives
through the macro. It is also why the live fork commit is `a1c741db` rather than
the `92c01732` 1035 records — that hash is not in the fork's history at all any
more (the patch line rebased), and the commit that survived is the one that
added the second half. `keyexpr.c:570-572` states it: "Defining the macro here
is the fix — changing only the comparison in the header is not, because the
`true` still arrives through the expansion."

### 3. Measured A/B — the pinned cross toolchain, the real export header

`arm-none-eabi-gcc 13.2.1` from the pinned SDK, `-fsyntax-only -mcpu=cortex-a7
-mfpu=vfpv3-d16 -mfloat-abi=hard -DZENOH_NUTTX -DZENOH_HAS_GETRANDOM`,
`-I third-party/nuttx/nuttx/nros-nuttx-export-arm/include -Iinclude`, on
`packages/rmw/zenoh/zpico-sys/zenoh-pico/src/session/keyexpr.c`:

| source | `error:` lines |
| --- | --- |
| the pin as it stands | **0** |
| the same file with its two call-site arguments put back to `true`/`false` (a copy under `tmp/`; the submodule untouched) | **11** |

The 11 are this document's opening errors, from the same header it names:

```
nros-nuttx-export-arm/include/stdbool.h:79:25: error: missing binary operator before token "1"
nros-nuttx-export-arm/include/stdbool.h:80:25: error: missing binary operator before token "0"
keyexpr_match_template.h:158:14: error: redefinition of '_z_chunk_special_includes'
   … 8 more `redefinition of '_z_*_includes'`
```

Host `gcc 11.4` with that same NuttX header shadowing its own `<stdbool.h>`
splits 0 vs 11 identically, so the result is a property of the header and not of
the cross toolchain. The negative control is what makes the green half mean
something: a compile that passes proves nothing about a defect it was never
able to see.

### 4. The build

`just nuttx build-fixtures-arm` — this document's original acceptance command —
run in a fresh worktree at this branch, with the arm kernel and export built
there (`just nuttx build`, `nros-nuttx-export-arm`, 2026-09-18). The
1039-relevant artifacts, by mtime:

```
02:34:45   25580  …/armv7a-nuttx-eabihf/nros-minsizerel/build/zpico-sys-*/out/*-keyexpr.o
02:36:55  690876  …/armv7a-nuttx-eabihf/nros-minsizerel/talker      (ELF 32-bit ARM, static)
02:36:37  690624  …/armv7a-nuttx-eabihf/nros-minsizerel/listener
```

`keyexpr.o` is the object this issue says cannot be produced; it exists, for
`armv7a-nuttx-eabihf`, dated to the run.

**Nineteen arm NuttX zenoh images LINKED**, zero `error:` lines in the whole
log: 7 Rust (talker, listener, service-{client,server}, action-{client,server},
logging-smoke), 6 C (`examples/qemu-armv7a-nuttx/c/*/build-zenoh/c_*`,
02:40) and 6 C++ (`…/cpp/*/build-zenoh/cpp_*`, 02:52). That also answers the
second box: `multiple definition of _z_socket_get_endpoints` was a LINK error,
and these link.

What that does NOT say: the lane as a whole. `build-fixtures-arm` continues into
the workspace fixtures (`workspace-fixtures-build.sh nuttx {rust,c,cpp}`), which
were still building when this was written, so the tick below is for the
stdbool/keyexpr class and not for a green lane. The class is settled by the 19
images and the A/B either way — every one of them compiles the same
`src/session/keyexpr.c` against the same non-conforming header.

One environment note, because it cost a run and looks exactly like a NuttX
build break while having nothing to do with one: in a worktree nested inside
its parent checkout (`<repo>/.claude/
worktrees/<id>`, which is where agent worktrees live), issue 1280's prefix
rewrite in `just/sdk-env.just` DOUBLES every defaulted path —
`replace(value, _NROS_OTHER, _NROS_HERE)` is applied to defaults that are
already rooted at `_NROS_HERE`, and `_NROS_HERE` contains `_NROS_OTHER` as a
prefix. `NROS_PLATFORM_POSIX_SRC` came out as
`…/agent-<id>/.claude/worktrees/agent-<id>/packages/platform/…` and the board
build script failed on a source file that does not exist. The shell spelling of
the same rule, `nros_reroot_checkout_path`, does NOT have the bug: it resolves
the value's OWNING checkout and keeps the value when the owner IS `here`. Two
spellings of one rule, and only one is prefix-safe. Worked around here by
dropping the inherited parent-checkout path variables before sourcing
`activate.sh`; it deserves its own issue.

### 5. Sweep: the class has exactly one site, and it is not ours

```bash
rg -n --glob '!third-party/**' --glob '!**/generated/**' \
   -e '^[[:space:]]*#[[:space:]]*(if|elif)[^\n]*\b(true|false)\b' \
   -g '*.c' -g '*.h' -g '*.cpp' -g '*.hpp' -g '*.cc' \
   packages examples cmake zephyr scripts | grep -v defined
```

**Zero hits.** Nothing we author uses `true`/`false` in a preprocessor
expression, so the non-conforming header reaches no other site of this class in
our own code — the third "Not verified" item, answered. Worth re-running on
every zenoh-pico pin bump, which is issue 1035's open item 2 and the only check
that would have caught this before the 1.8.0 merge.

### The remaining boxes

* **`multiple definition of _z_socket_get_endpoints`** — FIXED on `main`, and by
  the lead this document identified: `runner.rs` no longer defines `ZENOH_LINUX`
  alongside `ZENOH_NUTTX`. The comment at `runner.rs:2127-2153` records the
  reasoning ("the arm was right and the NAME was wrong"), the capability it
  actually wanted is now `ZENOH_HAS_GETRANDOM`, and
  `check-zenoh-platform-macros` gates BOTH producers — the platform manifests
  and `runner.rs`'s `build.define` — so the two authored copies of "what NuttX
  defines" can no longer disagree.
* **`nros-platform.toml` is a watched build input** — FIXED on `main`:
  `platform_manifests_to_watch` (`runner.rs:1061`) ENUMERATES the platform
  package directories instead of reconstructing `root/<name>/`, which is the
  reason the manifests under `packages/platform/` were watched by nothing. Its
  doc comment credits this issue, and names the consequence this document hit:
  an experiment on platform defines that measures the old defines and reads as a
  refuted hypothesis.
* **The root fix, or a documented decision** — decided below.

### The decision: carry the fork-side workaround, and do NOT add a second one

The fork-side fix (option 1 above) is the one that shipped, and it stays. Two
things recommend it over the other two options, now that both have been tried:
comparing against `1` is correct on a conforming `<stdbool.h>` too, so it forks
no behaviour by platform and is upstreamable as-is; and it sits in the ONE file
whose design depends on the macro being an integer constant.

**A second workaround existed and is deliberately not landed.** Commit
`a270ceb3e` — rescued from the unpushed branch
`backup/local-2026-09-11/wip/zenoh-linux-test`, where it was the only copy —
implements option 3: `nros-zpico-build` generates a conforming `true`/`false`
header into `OUT_DIR` and force-includes it (`-include`) ahead of every
zenoh-pico TU on NuttX, keyed on the target triple. Measured here: it
cherry-picks onto today's `runner.rs` with NO conflict (only the doc half
collides, add/add, with this file). Its author measured it taking the lane from
12 errors to 0 in 2026-09-04's tree; that was not re-measured, because there is
no longer a failure for it to fix. It is still the wrong thing to land:

* It is **redundant** — §3 shows the class cannot fire at this pin.
* It would be **unexercised**, and this tree's own lesson is that code no lane
  reaches is not code that works (the `check-lane-contracts` /
  `check-default-gates-run-somewhere` family). A defensive `-include` nothing
  compiles against is a claim, not a guard.
* It **redefines a vendored libc's `true`/`false` for a whole library's TUs** to
  work around one file's requirement, which the options list ranked last for
  exactly that reason: it papers over a broken header for one consumer and helps
  no other.

So: the workaround is the fork's, its scope is the file that needs it, and the
root fix remains an upstream NuttX matter — this document is the record of that
decision rather than a request for more work.

### Filing note: this is the third copy of one defect

The same defect was filed three times on 2026-09-04 — **1035** (canonical,
resolved, archived), **1047** (retired without reaching `main`) and this one,
which reached `main` two days later and stayed open. Nothing here duplicates
1035's value: the two follow-up items above were found only by this session's
diagnosis, both are now fixed, and the load-bearing call-site half of the fork
fix contradicts what 1035 wrote down. Resolved as 1035's sibling rather than as
its duplicate.
