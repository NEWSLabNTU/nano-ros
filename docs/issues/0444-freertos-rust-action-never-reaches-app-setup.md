---
id: 444
title: The FreeRTOS Rust action image boots and brings up the network, then never reaches application setup
status: open
type: bug
area: rmw
related: [issue-0442, issue-0350, phase-338]
---

## Symptom

`rtos_e2e::test_rtos_action_e2e` on `Freertos` / `Rust`:

```
freertos E2E failed — readiness pattern 'Application setup complete' not observed.
Output so far (truncated):
  ========================================
    nros FreeRTOS Platform
  ========================================
  Initializing LAN9118 + lwIP...
    MAC: 02:00:00:00:00:00
    IP:  192.0.3.10
  Network ready.
```

The image boots, the platform banner prints, LAN9118 + lwIP come up and the
interface gets its address. Then nothing. It stalls between "Network ready" and
application setup — so the platform is fine and the application never runs (or
blocks during registration / session open).

Fails SOLO, 3 of 3 retries, so it is not the QEMU-under-load flake class.

## This was hidden, and that is the interesting part

The cell has not been RUNNING. It reported `[SKIPPED] … STALE` against
`zpico-sys/c/include/zpico.h` — issue 0442, where one arm of the cmake freshness
probe did not apply the regenerated-header exemption its sibling did. Fixing
0442 made the cell execute, and it failed immediately.

So a probe defect was masking a runtime defect: the lane looked like a staleness
problem for as long as the fixture was never launched. That is issue 0350's
class exactly — a coordinate that never runs is indistinguishable from one that
cannot.

## Scope

`Freertos` + `Rust` only. On the same board and network, in the same run:

| cell | result |
| --- | --- |
| `Freertos::C` | pass |
| `Freertos::Cpp` | pass |
| `Freertos::Rust` | **fails at readiness** |
| `Nuttx::{Rust,C,Cpp}` | pass |
| `ThreadxLinux::{Rust,C,Cpp}` | pass |

8 of 9 action Runtime cells pass; this is the one.

## What is known and not known

* The entry wiring LOOKS intact: `src/main.rs` carries `nros::main!()` with
  `[package.metadata.nros.entry] deploy = "freertos"`, and `src/lib.rs` has the
  `register` body.
* The last commit to touch the package is `ab486a8db` (phase-338 W2, "collapse
  the 18 `-entry` packages into their node packages") — the same commit that
  dropped the NuttX boards' static link args (issue 0440). That makes it a
  natural suspect, but suspicion is not evidence and this has NOT been bisected.
* Not attributed to a branch. It was found on `phase-339-nuttx-export-snapshot`,
  whose changes are NuttX-only plus the 0442 probe fix; none of them touch the
  FreeRTOS runtime. A main comparison is the next step and has not been run.

## Next step

Run the cell on `main` with freshly built fixtures. If it fails there too this
is a main regression to bisect around `ab486a8db`; if it passes, the difference
is in this branch and the assumption above is wrong.
