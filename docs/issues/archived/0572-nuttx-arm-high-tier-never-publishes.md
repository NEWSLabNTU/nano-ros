---
id: 572
title: "nuttx-arm/rust realtime tiers: the 10 ms /ctrl tier delivers NOTHING
  while the 100 ms /telem tier works"
status: resolved
type: bug
area: platform-nuttx
related: [issue-0569, issue-0570, issue-0565, issue-0571, issue-0246, rfc-0015, phase-281]
---

## Symptom

`realtime_tiers_e2e`, cell `nuttx-arm/rust`, against a freshly built image:

```
[nuttx-arm rust] high-tier /ctrl counter 0 is not ≥3× the low-tier /telem
counter 4 — the 10 ms tier is not outrunning the 100 ms tier

--- /ctrl observer output (empty ⇒ nothing was received at all) ---

--- /telem observer output ---
Received: 0 … Received: 4
```

Not "too slow": **zero**. The fast tier's subscriber receives nothing at all
while the slow tier on the same image, same router, same run delivers five
samples. The two tiers are `[tiers.high]` (10 ms `/ctrl`) and `[tiers.low]`
(100 ms `/telem`) of RFC-0015 Model 1, driven by `QemuArmVirt::run_tiers`
(a std::thread per tier, phase-281 W3-nuttx).

## Reproduce

```sh
just nuttx build-fixtures-arm            # ~15 min, clean
./target/debug/deps/realtime_tiers_e2e-* --nocapture
```

Do NOT run it under `cargo nextest`: the suite exceeds the 60 s timeout once
embedded images exist and prints nothing at all (issue 0571). That is why this
cell was invisible — tier 1 has been reporting PASS by skipping it.

## What is known

* Reproduces on a from-scratch rebuild of the arm NuttX fixtures, so it is not
  a museum binary.
* The other 15 cells in the same run pass, including `nuttx-riscv`, so it is
  not the Model-1 seam in general.
* Discovered during phase-351 W3, whose diff cannot explain it: W3's only
  content change for this image is the `libc` `[patch.crates-io]` row moving
  from a hand-authored line to a sync-managed one with the IDENTICAL path
  (`cargo metadata` resolves `libc 0.2.183` →`third-party/nuttx/libc` either
  way). The cell had never actually run in the sessions before it.

## The guest console (2026-08-14, after the evidence gap below was closed)

```
nros entry ready
nros: multi-tier run — 2 tier(s) over one session
nros: tier priority set tier=`low` prio=100
nros: core pin FAILED tier=`low` cpu=0 — kernel lacks CONFIG_SMP, tier runs unpinned
```

Four lines, and only ONE tier in them. `low` is `tiers[1]`, a SPAWNED thread —
the Rust arm self-applies priority at tier entry, so a spawned tier prints that
marker. `high` is `tiers[0]`, the BOOT tier: it owns the session, keeps the
default Fifo SchedContext deliberately (issue 0246 — a budgeted context there
caps the shared zenoh-pico flush and starves delivery), and prints no marker.

So the spawned tier is healthy and the SESSION-OWNING tier publishes nothing.
Not a spawn failure (no `FAILED to spawn tier` line), not a session failure (the
session opened and `/telem` flows through it), and not the 0246 budget trap
(`[tiers.high.nuttx]` declares `budget_us`+`period_us`, so `boot_is_budgeted` is
true and `run_tiers` drops both for the boot tier — the mitigation is engaged).

## Evidence gap this had to close first

Issue 0565 taught the verdict to print the guest console — on the ONE path where
the symptom was noticed (the low-tier anchor). This failure takes the RATIO
path, which killed the guest *before* reading it, so the console was destroyed
by construction. Every verdict arm now drains through one
`guest_console(&mut guest)` helper before killing, which is how the four lines
above exist at all.

## Relationship to #569

Same cell, DIFFERENT console. #569 has these four lines PLUS `RMW session open
failed — ConnectionFailed` and an abort, so neither tier delivers. Here the
session opens and the low tier delivers five to eight samples. Either two
defects share a cell, or one root cause presents two ways depending on timing.
Whoever takes one should read the other.

## Narrowed, 2026-08-14 — it is the BOOT tier, and it never spins

Ten build/run cycles of in-guest instrumentation (all of it kept, see below).
The console now says:

```
nros: boot tier `high` (session owner) — groups ["ctrl"], class Some("real_time"),
      budget Some(5000) us, period Some(10000) us, spin 1000 us
[INFO] Control::register on a tier admitting group `ctrl`
[INFO] Telem::register on a tier admitting group `telem`
nros: spawning tier `low` — groups ["telem"], class None, spin 10000 us
nros: tier priority set tier=`low` prio=100
nros: core pin FAILED tier=`low` cpu=0 — kernel lacks CONFIG_SMP, tier runs unpinned
[INFO] Control::register on a tier admitting group `ctrl`
[INFO] Telem::register on a tier admitting group `telem`
nros: tier `low` entering spin — wake primitive available (16 byte(s))
[INFO] on_telem: first publish OK (tier `low` is dispatching)
nros: tier `low` completed spin 1
nros: tier `low` FIRST dispatch at spin 1 — 1 timer(s), 0 sub callback(s)
```

Established:

1. **`high` IS the boot tier** (`tiers[0]`, the session owner). Its knobs
   survived the bake exactly as `system.toml` declares them, and its groups are
   `["ctrl"]` — nothing is misrouted.
2. **`Control::register` RUNS on it.** The ctrl timer is registered on the boot
   executor, under a group that executor admits.
3. **The boot tier never completes a single spin.** `nuttx_spin_tier_forever`'s
   first statement prints `entering spin`; `low` prints it, `high` never does.
   So this is not "the timer fires and the publish fails" and not "the tier is
   slow" — the session-owning tier does not reach its loop.
4. The last thing it is seen doing is `spawn_scoped` for `low`. The child runs
   to completion of its own setup and spins happily on the SAME session.
5. **Not the sporadic budget.** `boot_is_budgeted` is true, so `run_tiers`
   already drops budget+period for the owner and keeps Fifo (issue 0246's
   mitigation) — verified from the printed knobs.
6. **Not console I/O.** Suspecting the parent was blocked writing to a console
   the child was using, every post-spawn write on the boot thread was removed
   and the run repeated: `/ctrl` still zero. (This ruled out a hypothesis that
   the earlier evidence supported — worth recording so nobody re-runs it.)

## A second defect, fixed on the way: stderr is a black hole on this guest

Every diagnostic in `run_tiers` was an `eprintln!` — a failed tier spawn, a
boot-tier setup failure, a spin error, the budget notice. **None of them can
ever appear on this guest's serial console**, which is why the `boot_is_budgeted`
notice was missing from a run where it must have executed. Issue 0565 taught the
HARNESS to capture that console for exactly these lines, and they were being
written to a stream that never arrives. All 11 sites in
`packages/boards/nros-board-nuttx/src/lib.rs` now write to stdout.

## What is NOT known

Why `spawn_scoped` does not return, or — if it does — why the boot thread makes
no further progress. That needs a debugger on the guest (or a NuttX-side thread
dump), not more printf: the next probe has to observe the parent thread's state
from outside, because everything it could print from inside is already either
printed or proven absent.

Whether this is a regression at all, and if so from when. Tier 1 has been
skipping this cell (issue 0571), so the last run that proves it working is
unidentified. **Do not bisect on tier-1 greens** — they do not carry
information about this cell.

The failure shape — one tier's publisher never producing while its sibling does
— is the same shape as archived issues 0144 (`run_tiers` tier-setup/declare
race) and #447/#458 (a registration race plus an unstamped handle tag), both on
the multi-tier path. Those are the first places to look; whether this is a
recurrence or a fourth instance is open.


## RESOLVED 2026-08-14 — the third symptom of #570's one bad write

`pthread_attr_init`/`pthread_attr_destroy` write NuttX's full 56-byte
`pthread_attr_t` into the 20-byte mirror the vendored `libc` fork declares
(`__PTHREAD_ATTR_SIZE__ = 5`, the `CONFIG_SCHED_SPORADIC=n` layout; both boards
set `=y`). On arm the object sits at `sp+0` of a 32-byte local area under
`push {r4, r5, r6, r7, r9, lr}` in `std::sys::thread::unix::Thread::new`, so the
overflow lands squarely on the pushed registers — including `lr`.

That is why the fast tier delivered NOTHING while the slow tier on the same
image and session worked, and the direction is the opposite of what "the spawned
one is the broken one" would suggest. `high` is the BOOT tier — it runs on the
main task and is the CALLER of `Thread::new`. The overflow lands on that
caller's saved registers, so the tier that gets corrupted is the one doing the
spawning, while `low`, which runs on the freshly created thread, is untouched
and publishes normally. The observation in this issue's own symptom section —
that the console names only `low` — is consistent with that: `low` reached its
self-apply path because nothing had corrupted it.

The "zero, not slow" shape was the tell: the tier's state was destroyed before
it ever published, not throttled. On riscv the same write hit `ra` instead and
the main task jumped to address 0 outright (#570); on arm it hit the same
function's `{r4, r5, r6, r7, r9, lr}` and execution limped on with garbage.

Fixed by `__PTHREAD_ATTR_SIZE__` 5 -> 14 (#570). `realtime_tiers_e2e` now
reports **16 row(s) ran, 0 skipped, 0 out of lane** and passes, `nuttx-arm/rust`
included, with no change to any tier, timer, or transport code.

#569, #570 and #572 were three separate issues written from three different
symptoms of one 36-byte overflow. Nothing in any of the three symptom reports
could have distinguished them; what did was `qemu-system-riscv32 -d exec,int`
naming the faulting instruction.

## Acceptance — met

* `nuttx-arm/rust` passes its `CounterRatio3x` proof (16/16 rows, 147 s);
* the cause names why `/ctrl` was zero rather than slow.
