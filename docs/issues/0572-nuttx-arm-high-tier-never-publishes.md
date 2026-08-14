---
id: 572
title: "nuttx-arm/rust realtime tiers: the 10 ms /ctrl tier delivers NOTHING
  while the 100 ms /telem tier works"
status: open
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

## What is NOT known

Whether this is a regression at all, and if so from when. Tier 1 has been
skipping this cell (issue 0571), so the last run that proves it working is
unidentified. **Do not bisect on tier-1 greens** — they do not carry
information about this cell.

The failure shape — one tier's publisher never producing while its sibling does
— is the same shape as archived issues 0144 (`run_tiers` tier-setup/declare
race) and #447/#458 (a registration race plus an unstamped handle tag), both on
the multi-tier path. Those are the first places to look; whether this is a
recurrence or a fourth instance is open.
