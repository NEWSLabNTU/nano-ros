# Phase 351 — Board facts and site config: split them, then retire the old path

**Status (2026-08-13). PROPOSED — no code landed.** Supersedes
[phase-349](phase-349-rtos-integration-shells.md) W2.0, whose leaf-`[env]`
carrier is retired here (W6).

**Implements:** [RFC-0072](../design/0072-rtos-integration-nano-ros-is-a-guest.md)
§5–§6, grounded in a survey of six real ecosystems (upstream FreeRTOS, ESP-IDF,
Pico SDK, STM32Cube, NXP MCUXpresso, Infineon MTB, Eclipse ThreadX + ST
X-CUBE-AZRTOS, PX4).

---

## The split

| | owner | lives in | holds |
| --- | --- | --- | --- |
| **A** | board package | `<pkg>/nros-board.toml` | identity, platform, target, capabilities, entry shape, toolchain file, `supported_netstacks`, flashing MECHANISM |
| **B** | user's project | `<bringup>/system.toml` `[deploy.<name>]` | SDK roots, chosen netstack, `config_files`, flashing PARAMETERS |
| **C** | test harness | harness config, keyed by board | QEMU invocations, timeouts |

`[deploy.*].board` is the join key. **No new file** — `[deploy.<name>]` already
exists with 59 blocks in the tree, nine in one workspace.

**Measured problem this fixes:** six of nine board descriptors carry site
content today (`runner`, linker scripts, `${workspace}` paths), and `${workspace}`
is stripped by a withholding filter that warns and *discards* — a filter with no
destination.

---

## W1 — `[deploy.*]` gains the site keys

- [ ] Schema + loader for `sdk.*`, `netstack`, `config_files` (a NAMED MAP —
      ThreadX needs `TX_USER_FILE` *and* `NX_USER_FILE`), `include_dirs`,
      `upload`, with `{env:…}` and `{sdk.*}` interpolation.
- [ ] `nros config explain` reports **file, section and rung** for every
      resolved value.

*Acceptance:* a `[deploy.*]` block carrying every key round-trips, and `explain`
names where each value came from.

**Nothing moves in this wave.** Additive only, so the tree keeps building.

**The explain output is the deliverable, not a nicety.** A site SSoT that cannot
say where a value came from just relocates the mystery — which is the whole
reason the current arrangement is hard to debug.

## W2 — this repo's own site values, generated and gated

- [ ] Generate the in-tree `[deploy.*]` site values from `just/sdk-env.just`.
- [ ] Both live; a gate asserts they agree (the phase-347 pattern — an
      agreement gate while two spellings coexist).

*Acceptance:* the gate fails when either side moves.

`just/sdk-env.just` is currently the de-facto site config *and only in-tree
users have it*. That is why an out-of-tree user had nowhere to put SDK roots.

## W3 — move site content OUT of board descriptors

- [ ] `runner` → test-harness config (C), keyed by board. It is duplicated
      across `baremetal` and `freertos` today because it describes the
      **machine**, not the board package.
- [ ] linker-script paths and `${workspace}` paths → `[deploy.*]`.
- [ ] **Delete the `${workspace}` withholding filter.** Once site paths have a
      home it has nothing to filter — that is the test that the split is real.

*Acceptance:* no board descriptor contains a path, a runner, or `${workspace}`;
`check-board-projections` still green; the FreeRTOS and NuttX fixtures build.

## W4 — `supported_netstacks`, and a resolver that checks it

- [ ] Board packages declare `supported_netstacks`.
- [ ] Selecting an unsupported pair is an error listing what IS supported.

**Not a free choice.** NetX Duo ships a smaller port table than ThreadX — 24
arches against 47 — so a ThreadX arch with no NetX counterpart **cannot be
paired**. The pairing has a validity domain.

## W5 — delivery, per lane

- [ ] cmake reads the resolved facts **via the CLI** (the `ws model-dims` seam).
- [ ] cargo build scripts receive them from whoever invokes cargo — one value,
      because exactly one board is active per configure (`if/elseif` on
      `NANO_ROS_BOARD`, and the toolchain file must precede `project()`).
- [ ] Zephyr uses the explicit export it already uses for knobs (issue 0460).
- [ ] **The gate:** a build whose deploy names a board must receive the facts,
      in every lane. Without it the board rung stays as silently dead as it has
      been since phase-290.

## W6 — retire the old path

- [ ] Remove the `[env] NROS_BOARD_TOML` row from the leaf projection
      (phase-349 W2.0). It points at the *descriptor*; the resolved facts
      supersede it, and it never reached workspace members anyway — corrosion
      runs cargo from `workspace_toml_dir`, so per-member config is invisible to
      it.
- [ ] Drop the now-dead `net_stack` ownership field, or give it a reader.
      Parsed and never read since it was added; wrong axis (`rtos-owned` /
      `nanoros-owned` answers *who brings up NIC+IP*, not *which stack*).
- [ ] `just/sdk-env.just` becomes a thin default over the site file rather than
      the source of truth.

*Acceptance:* `git grep NROS_BOARD_TOML` finds only history; the fixtures build.

---

## Order

```
W1 ──► W2 ──► W3 ──► W4 ──► W5 ──► W6
```

Strictly sequential: each wave's acceptance is the next wave's precondition, and
W6 removes a path W5 must first replace.

## Risks

* **W3 touches every board fixture.** Site content moving out of descriptors
  changes what the projection renders, so `check-board-projections` and the
  fixtures are the gate. Run a real build per platform family, not just the
  unit tests — phase-349 W1 renamed a config directory and no fixture has built
  against it yet.
* **W5's per-lane delivery is where this can rot.** Three mechanisms, and the
  failure mode is not a wrong value but **no value, defaulted, no diagnostic** —
  issue [0529](../issues/0529-zephyr-platform-knobs-never-resolve.md)'s shape,
  which took two wrong write-ups to characterise.
* **PX4 cannot be made per-board selectable.** Its configuration namespace is
  closed to out-of-tree code, so W4/W5 must not promise a `<label>.px4board`
  opt-in. RFC-0072 §6.6.

## Deliberately not here

* **A `netstack` provider kind with selection.** Every vendor has welded its
  choice; NXP's lwIP fork *contains* the ethernetif drivers. `supported_netstacks`
  plus capabilities is the whole of it.
* **`[board.memory]` constraints.** ThreadX/ST is the only evidence so far (a
  linker script jointly owned by board and driver). One case is not a schema.
* **Moving `cmake/board/*.cmake` selection off filenames.** Real, but orthogonal
  — it is board *delivery*, not board *facts*.
