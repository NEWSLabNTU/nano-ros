---
id: 1346
title: "`check-stack-floor`'s own coverage assertion is red on `main`, and because
  it runs per fixture ROW it takes the whole esp32 build down — the gate is
  lane-exempt, so nothing asked before the merge"
status: resolved
type: bug
area: [ci, tooling, embedded]
severity: high
related: [1344, 1071, 1226, 0413]
---

## What happens

```
$ just esp32 build-fixtures
  → examples/esp32-c3-baremetal/rust/talker
Traceback (most recent call last):
  File "scripts/check-stack-floor.py", line 505, in <module>
  ...
  File "scripts/check-stack-floor.py", line 360, in selftest
    assert not _missing, (
AssertionError: ROW_PLATFORM_BOARD does not name workspace platform(s)
['threadx-riscv64']. Add each — `None` when the platform has no stack floor —
so a row cannot escape this gate by not matching.
error: recipe `build-qemu` failed with exit code 1
```

Reproduced 2026-09-12 on `main` (`d4025ceba`). It is not an esp32 defect: the
esp32 rows are the only ones that call `check-stack-floor.py --row`, and the
script runs its own selftest before doing anything, so a red selftest is an
`exit 1` on a row that has already linked successfully.

## Why

`67450b315 test(#1286): a pure-C ThreadX workspace entry on rv-virt-threadx`
(2026-09-11) added a `[[workspace_fixture]]` with `platform =
"threadx-riscv64"`. `ROW_PLATFORM_BOARD` in `scripts/check-stack-floor.py` names
the other nine platforms and did not gain this one — which is exactly the
condition its phase-413 W2 assertion exists to catch, and the assertion worked.

The right value is `None`: `threadx-riscv64` gives a thread its own stack out of
the ThreadX byte pool (`tx_byte_allocate` in `threadx_hooks.c`), so the floor is
the PORT's `stack_bytes` (issue 0667), not a `_stack_start`/`_stack_end` linker
leftover. Same answer as its `threadx-linux` sibling, one line above.

## Why nothing caught it

`stack-floor` is in `.config/gate-lane-exempt.txt` — "in no lane and no caller
found — issue 1071's class" — so no `just check` lane and no workflow runs it.
Its only real caller is `scripts/build/fixtures-build.sh:462`, per ROW, inside a
fixture build that only the nightly esp32 lane performs. So the assertion fires
for the next person to build an esp32 image and for nobody before that.

The exempt note is now also WRONG on its face: there is a caller, it just is not
a `just` recipe. Issue 1071's ledger should record the caller rather than
"none found" — an exemption whose stated reason is false is how a gate stops
being anyone's.

## Fix

`"threadx-riscv64": None` in `ROW_PLATFORM_BOARD`, with the reason its
neighbours carry.

Verified: `python3 scripts/check-stack-floor.py --selftest` passes, and
`just esp32 build-fixtures` gets past the row check.

## Not fixed here

The lane exemption. Two separate reds (this and issue 1344) both reached `main`
because building one esp32 image is nightly-only; that is one problem with one
fix, and it is not a one-line change.
