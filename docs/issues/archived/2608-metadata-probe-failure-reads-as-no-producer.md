---
id: 2608
title: "A FAILED metadata probe build is reported as \"no producer\" and `nros configure` ends `sync: done`, exit 0"
status: resolved
type: bug
area: [cli, metadata, testing]
severity: medium
found: 2026-11-15
resolved: 2026-11-16
resolved_in: "phase-559 W11"
related: [phase-559, 0445, 0257, 1469]
---

**Owner:** nros 175. **Home:** phase-559 W11. Found while running phase-559
W10's smoke test; it is not fixed inside W10.

## What happens

`nros dev bindings`, and every `nros configure` that refreshes source metadata,
folds two different facts into one list, `RefreshReport::unsupported`
(`orchestration/metadata_refresh.rs`):

1. **A component nothing can probe:** its language has no producer, or it
   cannot be probed now. That is the fallback this list was made for (phase-307
   W3), and it is reported so the bake knows the entity count is unknown (issue
   0257).
2. **A component whose probe BUILD FAILED.** Its `Err(why)` is pushed onto the
   same list with the build error in parentheses.

`cmd/ws.rs` prints every entry as `sync: source metadata — no producer for
<pkg>::<comp> (<why>)`, then prints `sync: done.` and returns `Ok(())`.
Measured 2026-11-15 on `examples/workspaces/c`, in a worktree whose zenoh-pico
submodule was not checked out:

```
sync: source metadata — no producer for talker_pkg::talker (metadata probe build failed for `talker_pkg::talker` (exit 2):
  ... error: failed to run custom build command for `zpico-sys v0.6.0 ...`
  zenoh-pico source not provisioned at ".../zpico-sys/zenoh-pico". Run: nros setup --source zenoh-pico ...
sync: done.
```

Every probe in the workspace failed, and the command exited 0. A second run
failed every probe for a different reason (W10's shared build-dir check
refused it) and read exactly the same way.

## Why it matters

This is issue 0445's shape: a broken build is replaced by a message that
explains itself, and the caller sees green. A script or CI lane that runs
`nros configure` and checks its exit status learns nothing. The workspace then
bakes with every sidecar missing, which issue 0257 is about. The error text
is there, but it is worded as "no producer", which tells the reader that
nothing is wrong.

## Expected

- The two facts are reported apart. A failed probe is a FAILURE, named by
  node, with the first error line and where the full log is.
- A failed probe makes the command exit non-zero. At minimum, a run where
  EVERY probe failed must refuse rather than print `sync: done`.
- The real no-producer fallback (case 1) stays as it is: reported, not fatal.
- Issue 1469's "probe failed at this source last sync; unchanged" entry is a
  recorded failure too, so it follows the same rule.

## Where to look first

`metadata_refresh.rs`: the `Err(why) => report.unsupported.push(..)` arm in the
C/C++ batch, and the other `unsupported.push` sites that carry a probe error.
They need a `failed` list beside `unsupported`. `ws.rs`'s report block is where
the exit status is decided. Check who calls `refresh_stale_sidecars` and reads
`unsupported` (regenerate-bindings, fixture builds) before changing the exit
code: some may rely on sync never failing on a probe.

## Resolved (2026-11-16, phase-559 W11)

A failed probe is reported as a FAILURE, apart from "no producer", and the
exit status follows `NROS_PROBE_FAILURE` (issue 2520's knob, taken in by the
manager's ruling): `warn` by default for an interactive command, refusing a
run where no probe succeeded; `refuse` in every lane where nobody reads the
warning. Measured:

- `examples/workspaces/*` (18; the claim "every tracked workspace" was wrong,
  see the reach gap below) ran `nros configure`
  under `NROS_PROBE_FAILURE=refuse` on the W11 branch: all exited 0, 87 probes
  rebuilt, 0 failed, so no fixture lane goes red from this change.
- Broken on purpose (`#error` at the top of `workspaces/c`'s `Talker.c`):
  under `warn` the run exits 0, prints `probe FAILED for talker_pkg::talker:
  …Talker.c:1:2: error: #error broken on purpose…` with the marker as the full
  error, and its last line before `sync: done.` is `1 metadata probe(s)
  FAILED`. Under `refuse` it exits 1 with the same node and cause, through
  issue 1469's recorded-failure path (the source unchanged since the first
  run).
- The negative control: a no-producer component and a deploy-bound degrade
  are not failures under either policy (unit tests). A mutation that drops
  the `refuse` branch fails a test.

## Reach gap (2026-11-17)

The measurement above covered 18 roots, and the tree has 114 tracked
workspace roots, 53 of them with a C or C++ package. A component outside the
18, `packages/testing/nros-tests/bins/idle-wake-zephyr-mps2`'s
`nros_idle_wake_witness_c` (phase-521 W7), included `<zephyr/devicetree.h>`
and Zephyr's kernel and tracing APIs in its node source, so the HOST probe
could never compile it. Before W11 that read as "no producer"; after it,
`refuse` stopped `just zephyr build-fixtures` on main, every leaf unbuilt,
since the fixture build syncs that workspace whatever its filter. (It refused
under `warn` too: the workspace has one component, and a run where no probe
succeeded refuses.) The typed-carrier fix (#4106) was the same fallout one site
over.

The sweep below found one more, of a second shape:
`packages/testing/nros-tests/fixtures/image_config_ws`'s `probe_pkg::probe`
declared no entity at all (it only prints what its image compiled in), and a
probe that records nothing is a failed probe by design
(`nros_cpp_metadata_dump` returns -2, so the run exits 254). That stopped
both `workspace-image-config-*` linux rows.

Fixed in the change that follows this note:

- The witness keeps its node (`Witness.c`) host-compilable and moves the
  board measurement (tracing hooks, devicetree IRQ, cycle counter) into
  `witness_board_zephyr.c`, which its CMakeLists.txt adds only inside a
  Zephyr build; the probe builds `witness_board_host.c` instead. `refuse`
  stays strict, and the probe now records the witness's subscription and
  timer.
- `probe_pkg`'s component declares a 1 s timer that does nothing, so the probe
  records one callback slot. `plain` now builds against what that derives
  (`max_cbs` 1) instead of nros-node's builtin (4), which it only reached
  because its probe failed; `image_config_workspace.rs` asserts the derived
  value, and that `warn`'s stated 13 differs from both.
- The class check is `scripts/check-workspace-probes.sh`
  (`just check workspace-probes`): `nros sync` under `refuse` over EVERY
  tracked workspace root with a C/C++ package, so a new component the host
  probe cannot compile is red before it merges.

