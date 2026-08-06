---
id: 457
title: "Leaf .cargo/config.toml mixed sync-owned patches with authored board wiring — the sync half is now a gitignored sidecar"
status: resolved
type: tech-debt
area: build
related: [issue-0440, issue-0444, phase-338, rfc-0048, issue-0272]
resolved_in: phase-338
---

## Resolution

Sync's managed `[patch.crates-io]` block moved OUT of the leaf
`.cargo/config.toml` and into a gitignored sibling,
`.cargo/nros-managed-patch.toml`, reached by a second `include` entry. The
authored half of the file stays tracked.

That split is the one the file actually has. The filing proposed rendering the
WHOLE config from `nros-board.toml`'s `cargo_config` and gitignoring it; the
measurement below says that would have deleted content with no other home.

## What the measurement changed

The filing's premise was "~85% of the tracked set is a hand-maintained COPY of a
declared SSoT". Checked per line, against every tracked leaf config in the repo
(75, not the 46 under `examples/` the filing counted):

| | count |
|---|---|
| tracked leaf configs | 75 |
| resolvable to a board via their board-crate dependency | 54 (0 ambiguous) |
| of those, whose board declares a `cargo_config` | 48 |
| **whose content EQUALS that `cargo_config`** | **1** |
| a strict superset of it | 12 |
| differing in both directions | 35 |

The leaves legitimately override the board. `examples/qemu-arm-baremetal/rust/
serial-talker` replaces the board's QEMU `runner` with a `-serial pty` variant
and adds `[env] NROS_HEAP_SIZE` / `NROS_LINK_IP` / `ZPICO_NO_SMOLTCP`. Rendering
the board body over it would drop all four, and nothing in `nros-board.toml` can
express a per-example override — so "render + gitignore" needed a new authoring
surface before it could be safe. It is not a copy.

Two other corrections to the filing:

* **Deploy tokens are not board names.** `deploy = "rtic-mps2-an385"` is a
  deploy-profile name; the board is `baremetal`. `check-board-cargo-config-applied`
  matches `deploy` against `platform =`, which is why it only ever checked 8
  leaves. The mapping that does work is the entry's board-crate DEPENDENCY —
  54 leaves, no ambiguity.
* **Some tracked configs did patch generated msg crates.** The filing checked
  only `examples/` and concluded none did. Three outside it did:
  `packages/testing/nros-bench/wake-latency-cortex-m3` and
  `tests/simple-workspace/src/{talker,listener}` — and `tests/` was outside the
  gate's walk, which is why they were never flagged.

## The binding reason (which the filing had right in direction)

A tracked `config.toml` that sync writes into is wrong twice over:

1. It commits host-derived paths. The managed rows name `generated/` crates
   produced from the USER's ament install — the same reason `generated/` itself,
   the leaf `Cargo.lock` beside it, and `nros-patch.toml` are all untracked.
2. It churns. The row set moves as a leaf's dependency graph resolves, so every
   sync dirtied the worktree (an `nros-log` row appearing and disappearing
   across runs during this session).

Neither is fixable by gitignoring `config.toml`, because `[build] target`, the
QEMU `runner` and the link rustflags cannot be regenerated and must survive a
clone. Splitting on that seam fixes both without inventing an authoring surface.

## What landed

* `nros sync` writes its managed block to `.cargo/nros-managed-patch.toml` and
  maintains the `include` entry that reaches it. The entry is dropped when the
  managed set empties — cargo ignores a missing `include` SILENTLY, the failure
  mode #272 fought for the central file.
* Re-sync MIGRATES: a `# nros-managed` row left in `config.toml` by an older
  sync is evicted, so no leaf carries the patch twice. 46 tracked configs lost
  their managed block this way; no authored line was touched (verified by diff:
  only tagged rows and the emptied table header were removed, and a user `libc`
  patch survived).
* **Out-of-tree consumers keep the block inline** — `render_patch_config_with(…,
  sidecar: false)`. #272 deliberately gives them no `include`, and its test
  caught the first cut breaking that.
* Three stale hand-written offenders dropped their generated rows, all inert
  (each leaf path-deps its msg crate in `Cargo.toml`, so no patch was needed):
  wake-latency, and simple-workspace's talker/listener. A fourth,
  `packages/interfaces/rcl-interfaces`, patched `generated/nros-builtin-interfaces`
  — a path that has not existed since the tree gained its edition subdirectory
  (`generated/humble/…`); inert because that manifest has no targets.
* `check-cargo-config-tracked` gained the arm and now walks `tests/` too:
  **a tracked config must not patch an uncommitted `generated/` tree**, with
  `packages/interfaces/*` exempt because they commit theirs. Verified to fail on
  a planted row and pass when removed.

## Not done

`check-board-cargo-config-applied` stays. The filing expected it to become
unnecessary, but that only follows if the board body is rendered — which the
measurement rules out for now. Board-derived rustflags in a leaf are still a
hand-kept copy and can still drift, which is what 0440 was; the gate remains the
only thing standing between that copy and another ~3680-undefined-reference
link. Making the board contribution managed (rendered rows the leaf may
override, in the shape the patch block now uses) is the follow-up that would
retire it.
