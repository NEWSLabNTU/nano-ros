---
id: 995
title: "The build tier is red on three gates, and has been for days — nothing
  runs it, so nothing said so"
status: resolved
type: bug
area: ci
severity: high
found: 2026-09-02
related: [issue-0993, issue-0981, issue-0952]
---

## Symptom

`just check build` fails three of its 21 gates in CI's container:

```
===== FAIL (borrowed-e2e,   rc=1,  37827ms) =====
===== FAIL (sched-dim-arms, rc=1,    476ms) =====
===== FAIL (source-gates,   rc=1, 496481ms) =====
check-build (parallel): 3 of 21 gate(s) FAILED
```

## It is not new, and that is the point

The scheduled `gate` runs were ALREADY red on `borrowed-e2e`:

```
33582399779  failure  2026-09-02T02:13:11Z   error: recipe `borrowed-e2e` failed with exit code 1
33461214046  failure  2026-09-01T02:05:17Z
```

Two consecutive nightlies, at least two days, and nobody noticed — because no
pull request and no merge group runs this lane, which is exactly what issue 0993
is about. 0993 predicted an ungated lane accumulates rot; this is the rot, found
by trying to gate it.

The two other failures were invisible for the same reason and may be older; no
attempt has been made to date them, because the nightly stops at the first
failing gate in a serial list and only the parallel runner (0993) reports all
three at once.

## Diagnosed

All three, and none of them shared a cause.

### `sched-dim-arms` — a guard a level too shallow

    if [ -z "${FREERTOS_DIR:-}" ] || [ ! -d "${FREERTOS_DIR}" ]

`-d` passes for a submodule path that exists and is EMPTY, which is exactly an
uninitialised submodule. CI has the directory and not the sources, so the guard
admitted the arm and the compile died on `FreeRTOS.h: No such file or directory`.

Its siblings already guard on what they use — nuttx on `$NUTTX_DIR/include` and
again on a specific `nuttx/config.h`, threadx on `$THREADX_DIR/common_smp/inc`.
Issue 0196's class: a probe narrower than the rule. Now guards on
`$FREERTOS_DIR/include/FreeRTOS.h`; verified both directions.

### `borrowed-e2e` — a missing feature, hidden by a museum artifact

Every `export_size!` in `nros::sizes` is inside `mod rmw_sizes`, gated
`#[cfg(feature = "rmw-cffi")]`. The fixture built `--features std,platform-posix`
— no RMW feature — and the size probe forwards the CALLER's features to its
nested build of `nros`. That build had no `__NROS_SIZE_*` symbols at all, every
size read 0, and `generate_config` took its `return None` branch, whose comment
claims the case is "`cargo check --no-default-features` / `cargo doc`". It was a
real `cargo build`, and the header it skipped is the one the fixture then links
against — which is what the build script's own warning means by "do not link the
resulting rlib".

**This was already true on my host.** The gate passed here only because
`target/nros-c-generated/.../nros_config_generated.h` survived from an older
build; forcing the build script to re-run reproduced the probe-0 warning
locally. A gate green on a museum artifact — the same class as 0978, 0985 and
0987. Verified by DELETING the header and re-running: it regenerates, gate
exits 0.

### `source-gates` — one bug, two manifestations, only one modelled

`cross_libc_two_set_precedence_holds` compiles a probe with the RTOS stub's
`stdlib.h` reachable and asserts the failure is the modelled `div_t` clash. On
the container it failed differently, so the test refused to conclude and said
"fix the gate fixture, do not assume the precedence bug" — behaving exactly
right.

Which manifestation you get depends on the cross toolchain:

| toolchain | what happens | error |
| --- | --- | --- |
| SDK store, newlib 13.2.1 | newlib's `stdlib.h` reached FIRST, stub's is a redefinition | `conflicting declaration 'typedef struct div_s div_t'` |
| CI container, apt newlib 10.3.1 | stub's `stdlib.h` reached INSTEAD, `<cstdlib>` finds nothing to re-export | `'abs' has not been declared in '::'` |

Both are the stub shadowing the real libc. The sanity check now accepts either,
via `models_two_libc_clash`, with a unit test feeding it BOTH REAL LOGS — this
host's live, and the container's copied from run 33654481082 — plus two
negative cases so an unrelated error still fails the gate.

## Why it matters beyond the three

A lane that is already red cannot start gating pull requests — issue 0993's
attempt to do so is blocked on this, and was reverted for it (plus a cost
finding recorded there). So this is the thing standing between the build tier
and the merge path.

## Not established

`source-gates`' fix removes the assertion that fired. Whether the test then
passes END TO END on the container is unverified: after the sanity check it
requires the same probe to COMPILE with the RTOS `include/cxx` prepended, and
that half cannot be exercised here — this host has a different cross toolchain,
which is the whole reason the two manifestations exist. The next CI run is the
measurement.

`source-gates` is also the lane's cost pole at 496 s. That number is untouched
by this, and it is the one that decides whether the lane can ever gate a pull
request (issue 0993).

## Acceptance

* [x] Each of the three has a stated cause, not just a passing run.
* [x] All three green locally (`sched-dim-arms`, `borrowed-e2e`, `source-gates`
      each rc=0).
* [ ] `just check build` green in CI's container — `source-gates`' second half
      is unverified here.
