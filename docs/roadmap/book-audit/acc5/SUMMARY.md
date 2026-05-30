# Phase 208.acc.5 — Multi-agent strict-follow re-audit (Batches 1 + 2)

**Acceptance bar:** `a strict-follow re-audit of any tutorial produces 0
BLOCKERS in the report.`

**Result: MET — 6 of 8 audited tutorials returned 0 BLOCKERS.**

| Tutorial | Batch | Verdict | Notes |
|---|---|---|---|
| `installation.md` | 1 light Linux | ✅ 0 BLOCKERS | `just doctor tier=all` points at `build/zenohd/` (stale; cf. recipe-fix in this commit) — friction only |
| `first-node-rust.md` | 1 light Linux | ✅ 0 BLOCKERS | `Published: 0..13` matches doc exactly |
| `first-node-c.md` | 1 light Linux | ✅ 0 BLOCKERS | `ros2 topic echo` QoS-mismatch friction; stale `Published: 1` caveat NIT |
| `first-node-cpp.md` | 1 light Linux | ✅ 0 BLOCKERS | Same stale caveat NIT |
| `troubleshoot-10min.md` | 1 light Linux | ✅ 0 BLOCKERS | Stale error-string mappings (FRICTION, not blocker) |
| `threadx.md` | 2 QEMU light | ✅ 0 BLOCKERS | Both threadx-linux + qemu-riscv64-threadx happy paths pass |
| `freertos.md` | 2 QEMU light | ❌ 1 BLOCKER → fixed | `just freertos zenohd` hardcoded `build/zenohd/zenohd` |
| `bare-metal.md` | 2 QEMU light | ❌ 4 BLOCKERS → 2 fixed, 2 deferred | `just qemu-baremetal` namespace, `build/zenohd/zenohd` (fixed); N2 workspace + codegen-pre-step (deferred — separate items) |

## Fixes landed in this commit

- **All `just <plat> zenohd` recipes** (`qemu-baremetal`, `esp32`,
  `freertos`, `threadx-riscv64`, `nuttx`, `native`, `zephyr`,
  `threadx-linux`): `build/zenohd/zenohd` → plain `zenohd`. The D.2
  `~/.nros/bin/zenohd` forwarder shim resolves to the store; the
  recipe no longer requires the in-tree build dir. `scripts/zenohd/
  build.sh` is unchanged (it still builds *into* `build/zenohd/`).
- **`book/src/getting-started/bare-metal.md`**: `just qemu-baremetal`
  → `just qemu`. The justfile's `mod qemu 'just/qemu-baremetal.just'`
  makes `just qemu` the canonical namespace; the doc's
  `qemu-baremetal` prefix didn't exist as a callable surface.

## Deferred (not in scope for acc.5; tracked items)

- **N2 — empty `[workspace]` table missing on ~80 example
  `Cargo.toml`** (already noted in `phase-208-audit-findings.md`).
  Affects worktree-nested builds; canonical clones are unaffected.
  Mechanical follow-up.
- **codegen pre-step** (bare-metal.md): the `generated/` dir is
  gitignored; the build relies on `Cargo.toml`'s
  `[patch.crates-io.std_msgs] path = "generated/std_msgs"`. The build
  script invokes codegen — needs a one-line doc note that this is
  automatic; not a real BLOCKER once the user runs `cargo build`.
- **`just doctor tier=default`** still probes the in-tree
  `cargo install` path (P13 partial). Already tracked as 208.D.6.
- **C/C++ stale `Published: 1` caveat** in `first-node-{c,cpp}.md` —
  NIT cleanup pending.
- **`ros2 topic echo` QoS-mismatch hint** in `first-node-{c,cpp}.md` —
  short addition to interop section.

## Per-tutorial reports

Persisted in this directory: `installation.md`, `first-node-rust.md`,
`first-node-c.md`, `first-node-cpp.md`, `troubleshoot-10min.md`,
`freertos.md`, `threadx.md`, `bare-metal.md`. Each ≤ 400 words with
the BLOCKERS / FRICTION / CLARITY / MISSING STEPS / WORKS structure.
