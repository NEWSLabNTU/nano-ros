# Phase 208 Stage 1 — Batch 2 (QEMU light) — Summary

**Batch 2 = `freertos.md`, `threadx.md`, `bare-metal.md`.** Three parallel
read-only audit agents, each in its own git worktree, strict-following the
tutorial. Wall-clock cap 30 min. Per-agent reports in this directory.

## Aggregate verdict

**All three tutorials are blocked at step 1 (`nros setup …`)** by the same
host-side issue (N1 below), and **all three carry the same body-of-tutorial
schema drift** (P2 `config.toml` vs `nros.toml`) the prior synthesis already
flagged. With those two fixed, freertos still has a C-build path blocker
(P3 `px4-rs` not fetched). Beyond that everything is doc-only drift — banner
strings, IP/MAC values, board-crate names, missing preflight steps.

## Confirmations of the existing P-matrix

(Existing finding → which batch-2 tutorials reinforce it.)

| Finding | freertos | threadx | bare-metal |
|---|---|---|---|
| **P1** `NROS_PLATFORM_CFFI_INCLUDE` env not exported | ✓ (every Rust build fails before setup blocker bites) | ✓ | ✓ |
| **P2** `config.toml` → `nros.toml` schema drift | ✓ (every key wrong) | ✓ (every key wrong) | ✓ (every key wrong) |
| **P3** `px4-rs` submodule not fetched | ✓ (C-build path) | — | — |
| **P4** `zenohd` not on PATH after `nros setup --rmw zenoh` | — | ✓ | ✓ |
| **P6** Embedded host daemon not started | — | ✓ (no preflight) | ✓ (no preflight) |
| **P7** Output banner + `Published: 0` off-by-one | ✓ (fake banner) | ✓ (fake banner) | ✓ (fake banner) |
| **P11** Wrong board-crate names | — | ✓ (`nros-board-riscv64-qemu` → real `nros-board-threadx-qemu-riscv64`) | — |
| **P14** Misc per-page bugs | ✓ (FreeRTOS `[scheduling]` invented) | ✓ (riscv64 IP/MAC drift) | ✓ (`prefix` field invented; CIDR is in `ip`) |

## Net-new findings (add to the matrix)

### N1 — Installed `nros` CLI ≤ index requirement is not enforced

Every batch-2 tutorial dies at the first `nros setup …` call with:

```
Error: invalid SDK index nros-sdk-index.toml … unknown field 'shallow'
```

The host's `~/.nros/bin/nros` is **0.3.1**; the in-tree `nros-sdk-index.toml`
uses `[source.*] shallow = true` + `recursive = false` (Phase 197.2 +
Phase 207 follow-up) which the CLI rejects unless it's ≥ 0.3.2. The
installer script `scripts/install-nros.sh` pins `NROS_VERSION=0.3.7` but
its first guard is `if command -v nros … exit 0`, so a stale install
silently slips through.

**Hit by:** every tutorial that says "first run `nros setup`" — all of
Batch 2, very likely the rest of Batch 3–6 too.

**Fix candidates:**
- `install-nros.sh` compare `nros --version` against `NROS_VERSION` and
  re-install when it doesn't match (cheap, robust, no doc churn).
- Every tutorial that calls `nros setup` lead with `sh scripts/install-nros.sh`
  (doc churn, easy to forget on a copy-out template).
- A `nros setup` `--check-version` preflight in the CLI that detects an
  older parser handed a newer index and emits a real upgrade hint instead
  of `unknown field 'shallow'`.

The first option (install-nros.sh self-upgrade) is the smallest UX fix
with the biggest reach.

### N2 — Standalone-copy-out claim breaks under nested checkout

`cargo build` from `examples/<plat>/rust/<example>` inside a git worktree
under `.claude/worktrees/…` fails:

```
current package believes it's in a workspace when it's not;
workspace: /home/aeon/repos/nano-ros/Cargo.toml
```

The example `Cargo.toml` has **no empty `[workspace]` table**; cargo walks
up to the outer nano-ros workspace which lists the example. The outer
manifest's `exclude` doesn't cover the worktree path, so the example is
adopted into a workspace it was supposed to be standalone of. **Affects
freertos/Rust + freertos/build-fixtures + bare-metal/Rust.**

A real end-user clone *outside* nano-ros won't hit this (cargo can't walk
to a parent workspace it can't reach), so this is **environment-specific
to worktree-based audits + to in-tree nested checkouts**. But it does
contradict the README's "each example is a standalone copy-out template"
promise — a user vendoring an example **into another workspace** would
hit the same upward-walk and get adopted there too.

**Fix:** every `examples/<plat>/<lang>/<example>/Cargo.toml` ships an
empty `[workspace]` table. ~80 example dirs; mechanical, one-line per
file. (Phase 118 collapsed the example tree but didn't add this.)

### N3 — Banner text in docs is fictional

`bare-metal.md`, `threadx.md`, and `freertos.md` all show an expected
`nros <Platform> <Role> Talker` banner that doesn't exist in any
`src/main.rs`. The actual output is the runtime's structured `nros_info!`
lines: `Declaring publisher on /chatter (std_msgs/Int32)`, `Publisher
declared`, `Published: 0`, `Published: 1`, … (the **start at 0** is the
P7 off-by-one separately recorded).

**Fix:** delete the fake banners from the three tutorials; quote the
real first ≤ 5 log lines from each example's `src/main.rs`.

### N4 — `~/.nros/bin` not auto-PATH (cross-cuts the install path)

Every batch-2 agent had to manually export PATH. The install script
prints a hint but the tutorials don't reproduce it. Either:
- have install-nros.sh write a `~/.profile.d/nros.sh` (or similar) the
  user's shell picks up on next login; or
- every tutorial leads with `export PATH="$HOME/.nros/bin:$PATH"` (doc
  churn, same forget-risk as N1's doc-leads variant).

## Recommendation for the 208.B edit plan

The matrix in `phase-208-audit-findings.md` already lines up the
mechanical doc edits. Batch 2 narrows the priority order:

1. **N1 first** — until `nros setup` works, nothing else in any of
   batches 2–6 reaches a build step. One-line `install-nros.sh` change.
2. **P2** (`config.toml` → `nros.toml` + schema) — every batch-2 tutorial
   produces an un-parseable file as written. Per-tutorial edit; can run
   in parallel with N1.
3. **N2** (empty `[workspace]` in examples) — one-line per
   `Cargo.toml`; unblocks the worktree-based audit + a user vendoring an
   example into their own workspace.
4. **P7 + N3** (banners + off-by-one) — `s/Published: 1/Published: 0/`
   + drop fake banners. Cosmetic but visible.

Then the rest of the existing 208.B plan as written.

## Files preserved

- `tmp/book-audit/batch-2/{freertos,threadx,bare-metal}.md` — per-tutorial
  reports.
- This file — aggregate.
