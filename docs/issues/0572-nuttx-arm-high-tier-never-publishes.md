---
id: 572
title: "nuttx-arm/rust realtime tiers: the 10 ms /ctrl tier delivers NOTHING
  while the 100 ms /telem tier works"
status: open
type: bug
area: platform-nuttx
related: [issue-0571, rfc-0015, phase-281]
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
