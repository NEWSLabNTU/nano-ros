---
id: 444
title: The FreeRTOS Rust action image boots and brings up the network, then never reaches application setup
status: resolved
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

## RESOLVED 2026-08-06 — `ab486a8db` was the cause, and the suspicion was right

Root-caused and fixed in `946c70791`. **All nine FreeRTOS cells now pass**
({pubsub, service, action} × {Rust, C, C++}), so the 8-of-9 is 9-of-9.

The `-entry` collapse dropped THREE things from the FreeRTOS manifests. All
three came from one root cause: the merge script decided what to carry from the
entry manifest using incomplete rules.

**1. `ip` / `gateway` — the one that caused this symptom.** The script carried
only `rmw`, `domain_id` and `locator` out of the entry's
`[package.metadata.nros.deploy.*]` block. The FreeRTOS entries also set

```toml
ip = "10.0.2.15"
gateway = "10.0.2.2"
```

because `LWIP_DHCP` is **0** on this board (`config/lwipopts.h`) — the address is
BAKED, not leased. Without them the image fell back to the board default
`192.0.3.10 / 192.0.3.1` (`nros-board-freertos/src/config.rs`) and could not
route to the router at `10.0.2.2`. So it brought up lwIP, printed
"Network ready.", and sat there: no error, no retry, nothing to grep. Exactly
the reported symptom.

**2. `nros-platform` dropped.** The fallback guard was
`if "nros-platform" not in ntext` — a whole-file substring test — and the
freertos node manifest MENTIONS `nros-platform/platform-freertos` in two
comments, so it read as "already present".

**3. `locator` lost to the node's value.** The rule was "add keys the node
lacks"; when both blocks define a key the ENTRY's is the deployed one. The
per-role ports (7800 pubsub, 7810 service, 7820 action) lost to the node block's
generic 7447.

### How it was found

The next step this issue proposed — run on main with fresh fixtures — reproduced
it, but did not explain it. What explained it was rebuilding the PRE-collapse
entry from `ab486a8db^` as a control and booting both side by side:

```
control (entry):    IP: 10.0.2.15  → Application setup complete → Publishing: 'Hello World: 1'
collapsed (broken): IP: 192.0.3.10 → (nothing)
```

That one-line diff named `ip`/`gateway` after two wrong guesses (the missing
platform dep, then the locator — both real omissions, neither the cause of THIS
symptom).

### Why nothing caught it

Same shape as 0440: the collapsed manifest was valid TOML that cargo and
`nros sync` both accepted. A deploy overlay with no `ip` is legal — it just
means "use the board default", which is correct for boards that DHCP and fatal
for one that does not. The loss showed only at runtime, on one platform, as
silence.

### The pattern worth carrying

**Substring tests against a whole file** bit three separate times in phase-338:
`"[[bin]]" in text` matched a comment saying *"no [[bin]]"*; `"nros-platform" in
text` matched a comment; and a repair script matched
`[package.metadata.nros.deploy.*]` inside a `[features]` comment and appended
keys to the wrong table. Structured edits need anchored patterns (`^key =`, a
real table header), never `in text`.
