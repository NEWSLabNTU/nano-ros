# Phase 351 — Board facts and site config: split them, then retire the old path

**Status (2026-08-16). COMPLETE — W1–W6 landed, including W5's Zephyr arm.**

> That arm was committed "unverified — the lane skips (`west not found`)".
> Provisioning the workspace made it measurable and it turned out INERT for the
> Rust entry lane; reopened as issue 0605 rather than left as a green claim, and
> closed once the values were seen in the generated ninja. A wave whose subject
> is values arriving silently could not ship an arm that delivered nothing. Supersedes
[phase-349](../phase-349-rtos-integration-shells.md) W2.0, whose leaf-`[env]`
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

> **W3 corrected two thirds of that sentence** (2026-08-14). The `${workspace}`
> paths are REPO paths, not site content — they now render leaf-relative, and the
> filter is gone. The linker scripts are search-path NAMES, not paths. Only
> `runner` really is site content, and it is the one thing W3 did not move; the
> reason is under W3.

---

## W1 — `[deploy.*]` gains the site keys ✅ (2026-08-13)

- [x] Schema + loader for `sdk.*`, `netstack`, `config_files` (a NAMED MAP —
      ThreadX needs `TX_USER_FILE` *and* `NX_USER_FILE`), `include_dirs`,
      `upload`, with `{env:…}` and `{sdk.*}` interpolation.
- [x] `nros config explain` reports **file, section and rung** for every
      resolved value.

*Acceptance:* a `[deploy.*]` block carrying every key round-trips, and `explain`
names where each value came from.

**Nothing moves in this wave.** Additive only, so the tree keeps building.

**The explain output is the deliverable, not a nicety.** A site SSoT that cannot
say where a value came from just relocates the mystery — which is the whole
reason the current arrangement is hard to debug.

## W2 — this repo's own site values, generated and gated ✅ (2026-08-13)

- [x] Generate the in-tree `[deploy.*]` site values from `just/sdk-env.just`.
- [x] Both live; a gate asserts they agree (the phase-347 pattern — an
      agreement gate while two spellings coexist).

*Acceptance:* the gate fails when either side moves.

`just/sdk-env.just` is currently the de-facto site config *and only in-tree
users have it*. That is why an out-of-tree user had nowhere to put SDK roots.

## W3 — give the withheld content a home ✅ (2026-08-14)

**Landed, with two of its three items answered differently than planned.** The
wave was written from the *shape* of the descriptors; doing it measured what
each key actually is, and two classifications were wrong. Recorded here rather
than quietly re-scoped.

- [x] **`${workspace}` paths now RENDER, leaf-relative** — they were never site
      config. Every one points INSIDE the repo (`third-party/nuttx/libc`, a
      board's own `config/`), so relative to the leaf they are identical in
      every checkout, which is exactly issue 0463's rule for what may be
      committed. The plan's "move them to `[deploy.*]`" would have made repo
      facts into per-project settings.
- [x] **The withholding filter is deleted.** It withheld whole top-level TABLES
      (one `${workspace}` row in `[env]` took `THREADX_PORT` with it) and named
      them in a header — a filter with no destination. Now: `[env]` values are
      rendered `{ value = "<rel>", relative = true }`, everything else plain.
- [x] **`[patch]` rows are DELIVERED, not withheld** — as sync's `# nros-managed`
      inline rows, resolved against the patch AUTHORITY (a workspace's root, a
      standalone leaf's own dir). They cannot ride the projection: the leaf's
      `config.toml` always owns `[patch.crates-io]`, and a second one collides in
      `board_projection_conflicts`, which drops the `include` for the whole file.
- [x] **Two shell workarounds retired.** `scripts/build/nuttx-libc-patch.sh`
      re-appended the NuttX `libc` row after every sync, from BOTH fixture lanes,
      because sync used to strip it. Its own header said it existed "until the
      upstream CLI bug is fixed" — against `nros` 0.3.7, and the CLI has been
      in-tree since phase 218. The board declares that row once now.
- [x] `nros ws check-board-projections --write` — the gate regenerates, so a
      descriptor edit does not mean 42 full syncs.
- [ ] ~~`runner` → test-harness config~~ — **wrong destination; deferred to W5.**
      See below.

*Acceptance met:* `check-board-projections` green (42 leaves), and the FreeRTOS,
ThreadX-riscv64 (the `[env]` case) and NuttX-arm (the `[patch]` case) fixtures
build. Positive proof rather than absence-of-error for the last one: `cargo
metadata` in `examples/qemu-arm-nuttx/rust/talker` resolves `libc 0.2.183` to
`third-party/nuttx/libc/Cargo.toml` with no hand-authored row left in the leaf.

**Why `runner` did not move.** The plan said it was test-harness config (C).
Measured: `nros_tests::qemu` never reads cargo's `runner` — it builds its own
QEMU command, with the networking the runner lacks. The runner's live consumer
is the BOOK (`book/src/getting-started/freertos.md` runs the example with
`cargo run --release`; the bare-metal page documents that the runner boots
QEMU *without* networking and sends you to `just qemu talker` instead). So
moving it to the harness would delete a documented user flow and give the
harness something it does not want.

What it actually is: **how a host runs this board's image** — a deploy
convenience, category B, sibling of `upload`. It is duplicated across the
`baremetal` and `freertos` descriptors because both boards run on the same
MACHINE. Moving it needs the delivery question W5 owns, and one W3 could not
answer: a site value has TWO homes — a workspace's `system.toml`
`[deploy.<n>.nros]`, and a standalone leaf's `Cargo.toml`
`[package.metadata.nros.deploy.<n>]` (which is where the 19 copy-out examples
declare `locator`/`ip`/`gateway` today).

**Not moved either, and deliberately:** the linker-script arguments. `-Tlink.x`,
`-Tdramboot.ld`, `-Tmps2_an385.ld` are NAMES resolved through the linker's
search path, which the board crate's build script propagates — not paths. There
was nothing there to move.

**Still a fourth copy:** `packages/testing/nros-tests/bins/logging-smoke-threadx-riscv64/`
hand-authors the same four `[env]` rows. It declares no
`[package.metadata.nros.entry] deploy`, so no descriptor claims it and sync
cannot reach it. Fixing that is a deploy-key question, not a rendering one.

## W4 — `supported_netstacks`, and a resolver that checks it ✅ (2026-08-15)

- [x] Board packages declare `supported_netstacks`, ordered (first = default),
      each from evidence in that board's own tree: freertos `["lwip"]` (its
      platform manifest builds `system/freertos/lwip/network.c`), mps2-an385 and
      esp32 `["smoltcp"]`, both threadx `["netxduo"]`.
- [x] An EMPTY list is a statement, not an omission — nuttx / linux / zephyr
      make no choice (their RTOS or host owns the stack), and naming one there
      is refused rather than ignored, because a silently dropped key leaves the
      deploy believing it selected something.
- [x] `BoardDescriptor::resolve_netstack` returns the resolved stack or an error
      LISTING the domain. 5 unit tests, one of which runs over every shipped
      descriptor so the declarations cannot rot.
- [x] `check-site-config` reads the domain from the descriptors instead of the
      board→netstack column it carried since W2 — that column was a second SSoT
      for a board FACT. Mutation-verified.

**Found doing it:** `[deploy.*].board` and a descriptor's `names` are NOT the
same vocabulary. Every in-tree site block says `board = "mps2-an385-freertos"`,
which that descriptor does not list (`freertos` / `freeRTOS` / `FreeRTOS`), so
`nros sync` cannot resolve those deploys either — phase-341 W3 closed part of
this gap and this part is still open. The gate accepts the directory spelling as
well rather than reporting a board it can plainly see as unknown.

**Not a free choice.** NetX Duo ships a smaller port table than ThreadX — 24
arches against 47 — so a ThreadX arch with no NetX counterpart **cannot be
paired**. The pairing has a validity domain.

## W5 — delivery, per lane ✅ (2026-08-15; Zephyr arm closed 2026-08-16, issue 0605)

- [x] `nros ws board-facts <ws> --board <name>` resolves ONE deploy's board rung
      + site config and prints `KEY=VALUE` lines: `NROS_BOARD`,
      `NROS_BOARD_TOML`, `NROS_NETSTACK` (W4-validated), `NROS_SDK_<NAME>`,
      `NROS_CONFIG_FILE_<ROLE>`, `NROS_INCLUDE_DIRS`, `NROS_DEFINES`,
      `NROS_UPLOAD_<KEY>`. One implementation, shared by every lane — the
      `ws model-dims` seam, not a second resolver in cmake.
- [x] cmake: `nros_board_facts_env(<target>)` (`cmake/NanoRosBoardFacts.cmake`),
      the sibling of `nros_cargo_profile_env`, attaches them with
      `corrosion_set_env_vars` — the target's own build command, because
      `set(ENV{})` reaches only the configure-time process (issue 0460).
- [x] Zephyr: the module resolves once at module scope, `nros_cargo_build()`
      carries them for the C/C++ core crates, and the RUST entry lane gets them
      through a `cargo-features-patch.sh` hunk that injects
      `${NROS_BOARD_FACTS_ENV}` into zephyr-lang-rust's own `cmake -E env`
      command (issue 0605 — that arm shipped INERT and was only measurable once
      the workspace was provisioned). Verified in the GENERATED ninja:
      `NROS_BOARD_TOML=…/packages/boards/zephyr/nros-board.toml NROS_BOARD=zephyr`.
      Still unmeasured: the C/C++ Zephyr cells, which issue 0590 stops before
      they configure.
- [x] Gate `check-board-facts-delivery`: every `corrosion_import_crate()` must
      be followed by the helper, with an EXEMPT map that states why a file
      carries no board. Mutation-verified.

*Measured:* `== freertos == OK` with
`nano-ros: board facts for 'mps2-an385-freertos' — 5 value(s) delivered to cargo`.

**Found doing it:** two deploy blocks may name the SAME board —
`examples/workspaces/mixed` has `[deploy.freertos]` and
`[deploy.mps2-an385-freertos]`, both on `mps2-an385-freertos`. That is only
ambiguous if they RESOLVE differently, so the verb compares the ANSWERS and
refuses only a real disagreement, naming the keys that differ. Refusing on the
key count alone would have blocked every cmake build of that workspace over a
distinction with no consequence.

## W6 — retire the old path ✅ (2026-08-15)

- [x] The `[env] NROS_BOARD_TOML` row is gone from the projection, and
      `merge_env_row` — the helper that existed only to splice it — with it.
      W5 delivers the same value from the invoker, which reaches workspace
      members the row never could. The standalone lane
      (`scripts/build/fixtures-build.sh`) exports it too, so the leaves that DID
      read the row keep the rung: `nros ws board-facts` learned the second site
      home for them (a copy-out example declares
      `[package.metadata.nros.entry] deploy = "<board>"`, where the deploy KEY
      is the board — those manifests carry no `board =` at all).
- [x] `net_stack` and its `NetStack` type are deleted. RFC-0064 had already
      concluded "delete rather than extend" and left an `[OPEN]` to check for a
      reader first; there was none, for either meaning it carried. That item is
      now closed in the RFC, which also re-points its cost axis at
      `supported_netstacks`: a board that declares stacks supplies them,
      an empty list means the host ecosystem does.
- [x] `just/sdk-env.just` — **already the thin default**, measured rather than
      changed: all 23 exports are `env("X", <repo-relative default>)`, so an
      override wins and the site file's `{env:VAR}` resolves through it. Making
      `just` READ the site file was considered and rejected: it would add a CLI
      call to every recipe to remove a drift the W2 agreement gate already
      prevents.

*Acceptance:* `git grep NROS_BOARD_TOML` finds only the zpico build script that
READS it (the consumer, which is the point) and history;
`check-board-projections` green over 47 leaves; `check-cargo-config-tracked` and
`_require-leaf-includes` green after six threadx-linux projections dropped out
(that board declares no `cargo_config`, so with the row gone it projects
nothing — and no include was left naming a deleted file, which is issue 0463's
invariant).

---

## Order

```
W1 ✅ ──► W2 ✅ ──► W3 ✅ ──► W4 ✅ ──► W5 ✅ ──► W6 ✅
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
