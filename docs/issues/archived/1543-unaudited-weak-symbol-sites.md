---
id: 1543
title: "Three weak-symbol sites no audit list covers, because the gate reads one spelling under `packages/**` only"
status: resolved
type: bug
area: [build, platform]
severity: low
found: 2026-09-28
resolved: 2026-09-28
related: [phase-472, ]
---

## What happens

VERIFIED present on `main`, none in `check-weak-symbols.sh`'s audited allowlist:

- `packages/boards/nros-board-freertos/c/freertos_c_entry.c:155` —
  `__attribute__((weak, used)) void zpico_set_task_config(…)`
- `zephyr/heap_stub_native.c`
- `zephyr/cyclonedds-zephyr/link_stubs.c`

The gate reads `git ls-files 'packages/**'` and matches the exact spellings
`__attribute__((weak))` and `.weak `. So `(weak, used)` is unmatched, and `zephyr/`
is unread. `__weak`, `#pragma weak` and `[[gnu::weak]]` are unmatched too.

## Why it matters

A weak symbol is a silent link-time override: the build succeeds whichever
definition wins. The allowlist exists so each one is a decision someone made.

## Fix

Audit the three sites; widen the gate to all owned C/asm and match
`__attribute__\s*\(\([^)]*\bweak\b`, `__weak` and `#pragma weak` (phase-472 W5).

## Resolution (2026-09-28)

**Gate widened** (`scripts/check-weak-symbols.sh`). It now enumerates every
tracked C/C++/asm file with no pathspec, minus the same vendored, build and
generated dirs. `zephyr/` and `examples/` are included, and submodule sources
are never listed. It matches `__attribute__ ((... weak ...))` in any attribute
list, `__weak`, `#pragma weak`, `[[gnu::weak]]` and asm `.weak <sym>`. A grep
error no longer counts as zero. A tree-reach floor fails the gate if no owned
file under `packages/` or `zephyr/` is enumerated, so a pathspec creeping back
would be caught.

**The three sites were audited and all are correct as weak.** No source was
changed. Each has a row in `scripts/weak-symbols-allowlist.txt`:

* `packages/boards/nros-board-freertos/c/freertos_c_entry.c`,
  `zpico_set_task_config`: `body:correct optional-hook`. A DDS/XRCE-only image
  has no zenoh tasks to tune. A zenoh image links `zpico.c`, whose strong
  definition overrides. It is not image-checked, so the case where a zenoh
  image loses that override (and ignores issue 0623's priorities) is not gated.
  It has not been observed.
* `zephyr/cyclonedds-zephyr/link_stubs.c`, `nsos_adapt_getifaddrs`:
  `body:reports-failure optional-hook`. The strong definition is the NSOS
  trampoline that `nsos-getifaddrs-patch.sh` adds. `-1` is the real function's
  own "no interface" return, and the caller falls back to the net_if walk and
  then to loopback.
* `zephyr/heap_stub_native.c`, `__heap_start`/`__heap_end`:
  `body:reports-failure optional-hook`. These are link-only symbols for
  picolibc `sbrk` on native_sim, where Zephyr's malloc wins and `sbrk` is not
  called. That a call would fail cleanly (ENOMEM, then NULL) is REASONED from
  the two one-byte objects, not measured.

**Selftest.** It runs on every invocation, so the script leaves the
gate-selftest baseline. It has 11 cases, all driven through the gate's own
filter and counter:

* each spelling;
* an unlisted `(weak, used)`, which must FAIL;
* a weak symbol in an unlisted file under `zephyr/`, which must FAIL;
* count drift;
* the vendored exclusion;
* identifiers named `weak` or `my__weak`, which must not count;
* the positive control.

**Measured on the real tree.** The audited tree passes: 20 files, about 5 s.
Appending `__attribute__((weak, used))` to
`packages/api/nros-c/c-stubs/log_fmt.c` fails, and so does appending it to
`zephyr/cyclonedds-zephyr/environ_zephyr.c`. Each failure names the file as a
NEW unaudited site.
