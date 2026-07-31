---
id: 361
title: "Grandfathered committed SystemModels: the current nros-launch-resolve requires explicit per-block `nodes=`, so any embedded-workspace re-resolve fails 'node not placed' (and c/mixed embedded entries have no board deploy)"
status: resolved
type: bug
severity: medium
area: codegen
related: [phase-296, issue-0320, issue-0355, rfc-0060]
resolved_in: "rlm 92c1a52 + Part-2 (2ce930e39)"
---

## UPDATE (2026-07-31) — the real root cause is broader

The symptom below (c/mixed embedded entries codegen-fail "no nodes on board") is
one face of a bigger contract break. Attempting the fix — add `[deploy.freertos]`
/ `[deploy.nuttx]` / `[deploy.threadx-linux]` to `examples/workspaces/c` and
re-run `nros ws sync` — fails at model RESOLUTION, before codegen:

```
Error: system config: node '/listener' is not placed — with multiple [deploy.*]
       blocks every node needs a `nodes = [..]` entry
       (packages/cli/.../ros-launch-resolve/resolve/src/model.rs:222)
```

**And it is not specific to the edited workspace.** Forcing the UNMODIFIED
`examples/workspaces/rust` workspace to re-resolve (any `system.toml` change →
content-addressed staleness → real re-run) produces the identical error, even
though its committed model is checked in and "works". So:

- The committed portable SystemModels (`#320`) are **grandfathered** — generated
  by an older resolver that placed nodes across `native` (default launch) +
  `robot1`/`robot2` (multihost) + embedded blocks WITHOUT explicit `nodes=`.
- The CURRENT `nros-launch-resolve` requires `nodes = [..]` on every deploy block
  once there is more than one. No committed embedded workspace declares them.
- Therefore ANY re-resolve of these workspaces fails — adding an embedded deploy
  target is just the first thing that forces one. `nros sync --check` /
  regeneration is effectively frozen against the committed models.

`kind = "embedded"` blocks are documented (rust `system.toml`) as running EVERY
node and being excluded from placement, so the ambiguity the resolver trips on is
between the `kind = "self"` machines: `native` (default `system.launch.xml` =
talker+listener) vs `robot1`/`robot2` (`multihost.launch.xml`, talker@robot1 /
listener@robot2). Those are ALTERNATIVE deployments of the same nodes; the older
resolver resolved it, the current one demands explicit partitioning.

**Fix reframed** — two real options, both bigger than a config add:
1. **Resolver** (`nros-launch-resolve`, vendored fork): restore placement for the
   `native`(default) + `robot*`(multihost) + `embedded` pattern without demanding
   explicit `nodes=` — treat multihost self-blocks as alternative placements of
   the default nodes, as the grandfathered models were built. Preferred: it
   unfreezes regeneration for every embedded workspace at once.
2. **Model authoring**: add explicit `nodes = [..]` to every deploy block in every
   embedded workspace (c, cpp, mixed, rust, ws-realtime-*). Large, and the
   alternative-deployment semantics (a node on both `native` and a `robot*`) still
   need the resolver to accept "machines are alternatives, not simultaneous".

Then the ORIGINAL c/mixed gap (below) is fixed by adding the embedded deploy
targets, which will finally resolve. Verification needs the embedded toolchains
(a full `just {freertos,nuttx,threadx} build-fixtures`).

---

## Original finding (2026-07-31, surfaced building tier-2 fixtures for #355)

## Finding (2026-07-31, surfaced building tier-2 fixtures for #355)

`just freertos build-fixtures` (and `nuttx`, `threadx`) fails at CMake
configure with:

```
CMake Error at cmake/NanoRosEntry.cmake:668 (message):
  nano_ros_entry(LAUNCH ""): `nros codegen entry` failed (rc=1).
  ...
  stderr: SystemModel `examples/workspaces/c/src/demo_bringup/config/system_model.yaml`
          places no nodes on board `mps2-an385-freertos` — check execution.deploy targets
```

Reproduces directly against the committed model with a freshly built CLI
(`just setup-cli`):

```
nros codegen entry --lang c --workspace examples/workspaces/c \
  --model examples/workspaces/c/src/demo_bringup/config/system_model.yaml \
  --board mps2-an385-freertos --out /tmp/e.cpp --typed
# Error: SystemModel ... places no nodes on board `mps2-an385-freertos`
# nros-cli-core/src/codegen/entry/mod.rs:456
```

## Root cause

`examples/workspaces/c` contains a `src/freertos_entry/` Entry package (and the
mixed workspace likewise), migrated to the model path in phase-296-R4/M1
("migrate the 15 monolith embedded entries to the model path", 31e051009). But
the workspace's bringup declares **only native deploy targets** —
`examples/workspaces/c/src/demo_bringup/system.toml` has `[deploy.native]`,
`[deploy.robot1]`, `[deploy.robot2]`, all `target = "x86_64-unknown-linux-gnu"`
— and the committed `system_model.yaml` (regenerated portable in #320,
07650d0a1) reflects exactly that: `/talker` + `/listener` on `target: linux`,
nothing on any embedded board.

So `codegen entry --board mps2-an385-freertos` filters the model's deploy
targets by that board, finds none, and fails — correctly, given the model. The
Entry package exists and the tier-2 lane tries to build it (`workspace-mixed-freertos`
is a declared `fixtures.toml` coordinate), but the model has no deploy target for
its board. The **rust** freertos fixture is unaffected because it lives in a
different workspace (`examples/workspaces/rust`, fixtures.toml
`workspace-rust-qemu-freertos`, entry `qemu_freertos_entry`) whose model does
deploy to the board.

Net: the C/mixed embedded Entry packages were migrated to consume the workspace
SystemModel, but the model (and its `system.toml` source) was never given the
embedded-board deploy targets those entries require — an entry that consumes a
model, paired with a model that deploys nothing to the entry's board.

## Impact

- `just {freertos,nuttx,threadx} build-fixtures` cannot build the C/mixed
  embedded workspace fixtures → tier-2/tier-3 `ci-matrix` cannot go green for
  those coordinates on a dev box. Distributed nightly likely hits the same wall.
- Not caught earlier because full tier-2 fixture builds are rarely run locally
  (the per-platform toolchains do not coexist on one runner), so this lane's
  breakage sat latent since 296-R4/#320.
- Unrelated to #355 (that fix is nros-c executor-spin logic); this is the
  workspace-model deploy shape.

## Fix direction

Either:
1. Add the embedded-board deploy target(s) to the C/mixed workspace bringup
   (`system.toml` `[deploy.*]` for `mps2-an385-freertos` and the nuttx board),
   so the regenerated SystemModel places the entry's nodes on that board — the
   shape `ws-realtime-c-mps2` already uses for its freertos tiers is the working
   template; then re-commit the portable model (issue-0320 flow); or
2. If the C/mixed freertos/nuttx Entry packages are vestigial (only the rust
   workspace is a real embedded fixture), remove them and drop the corresponding
   `fixtures.toml` coordinates + matrix cells so nothing tries to build them.

The distinguishing question — are C/mixed embedded workspace fixtures INTENDED
coverage? — decides which. `workspace-mixed-freertos` being a declared
`fixtures.toml` row argues for (1).

## Repro

```
just setup-cli
nros codegen entry --lang c --workspace examples/workspaces/c \
  --model examples/workspaces/c/src/demo_bringup/config/system_model.yaml \
  --board mps2-an385-freertos --out /tmp/e.cpp --typed
```

## Progress (2026-07-31) — resolver fix DONE (fork-local), rest is follow-up

**Two red herrings cleared first:** (1) `nros ws sync` shells out to the STANDALONE
`nros-launch-resolve` binary (RFC-0060), rebuilt by `just setup-launch-resolve`,
NOT by `just setup-cli` — every earlier "still fails" was a stale resolver binary.
(2) The embedded-exclusion fix (`984fc15`, "embedded blocks do not partition") was
already present; with a freshly built resolver, `nros ws sync` on a
native+embedded workspace RESOLVES.

**Remaining real bug + fix (resolver):** `984fc15` stops embedded blocks from
partitioning, but a placed node was then pinned to its self-block's concrete
target (`linux`). The model deploy is single-target per node, so `codegen entry
--board <embedded>` (whose `keep()` only admits a `linux` node for `native`/`posix`)
found nothing on the embedded board. Fix: when ANY `kind="embedded"` block is
present the placement is board-AGNOSTIC (`target = None`); `keep()` admits a `None`
node on every board, and each entry's `--board` supplies the concrete target.
Single-board workspaces are unchanged.

- Committed in the vendored fork `ros-launch-manifest` as `b3d82d3` (on top of
  `origin/main`/`984fc15`), with regression test
  `embedded_blocks_make_placement_board_agnostic` (both directions). 37 manifest
  tests green. **Not pushed** — vendored-fork exfiltration rule; the maintainer
  pushes the fork chain, then bumps the superproject pointers
  (`ros-launch-manifest` → `ros-launch-resolve` → nano-ros).
- **Verified locally** (fork built into the resolver): `examples/workspaces/c` with
  `[deploy.{freertos,nuttx,threadx-linux}]` added → `nros ws sync` succeeds, the
  regenerated model places `/talker`+`/listener` board-agnostic, and `codegen
  entry --board {native,mps2-an385-freertos,nuttx-qemu-arm,threadx-linux}` all pass
  the placement check (the "no nodes on board" error is gone).

**Follow-up (Part 2, separate) — still open:**
1. Push the fork chain + bump the superproject pointers (maintainer).
2. Add `[deploy.{freertos,nuttx,threadx-linux}]` to the c / cpp / mixed workspace
   bringups (`system.toml`) for their declared embedded fixtures
   (`workspace-{c,cpp,mixed}-{freertos,nuttx,threadx-linux}`), then re-`nros ws
   sync` to regenerate the portable committed models.
3. Regenerate EVERY embedded workspace's committed model against the fixed
   resolver (they are grandfathered) + re-commit; the `resolve-fingerprint` gate
   will flag them stale.
4. Verify `just {freertos,nuttx,threadx} build-fixtures` builds green (needs the
   embedded toolchains).

## RESOLVED (2026-07-31)

(Filed as #356; a concurrent agent's px4 issue took 0356, so this was renumbered
to #361 — the pushed commits reference it as **#356**.)

Landed end-to-end:
- **Resolver fix** (fork chain): `ros-launch-manifest` `92c1a52` — a multi-board
  system (self machines + `kind="embedded"` board builds) places nodes
  board-agnostically (`target = None`) so `codegen entry --board <embedded>`
  includes them; `984fc15` alone (embedded blocks don't partition) fixed only the
  resolve, not the codegen. Regression test added. → `ros-launch-resolve`
  `69c13d2` → nano-ros `f760141ef`.
- **Part 2** (`2ce930e39`): `[deploy.{freertos,nuttx,threadx-linux}]` authored in
  the c/cpp/mixed bringups; portable SystemModels regenerated for every
  multi-board workspace against the fixed resolver.

Two red herrings cleared: `nros ws sync` runs the STANDALONE `nros-launch-resolve`
binary (`setup-launch-resolve`, not `setup-cli`) — earlier "still fails" were
stale-binary; and `984fc15` was already present.

**Verified:** `just freertos build-fixtures` codegen no longer errors "places no
nodes on board" — the blocker is gone; the build proceeds to compilation.

**Residual (separate, NOT this issue):** the freertos build then fails
`nros/app_config.h: No such file` with `nros-board-mps2-an385-freertos` at `v0.4.0`
(version-lockstep mismatch) — a board-build issue, a fresh follow-up.
