---
id: 363
title: "A stale `nros` binary silently emits a WRONG `[patch.crates-io]` table — the staleness guard exists but a direct `nros sync` bypasses it"
status: resolved
type: bug
severity: high
area: build, cli
related: [issue-0320, issue-0359, issue-0197, rfc-0048]
---

## Correction first

The original filing said *"a generator that writes before it can finish"* and
recommended atomicity. **That was wrong on both counts**, and is preserved here
rather than quietly replaced, because a wrong diagnosis points at the wrong fix:

- `write_patch_config` and `write_central_patch_file` are **already atomic** —
  temp file plus `rename` (`ws.rs:2276`, `ws.rs:1777`).
- The write was not interrupted. `write_patch_block` runs to completion at
  `ws.rs:1113` and `refresh_source_metadata` fails afterwards at `ws.rs:1123`.
  The file was written completely, and written **wrong**.

Atomicity would have changed nothing. The corrupt output was the honest result of
a corrupt input.

## What actually happened (2026-07-31)

`nros sync` dropped a `# nros-managed` entry from a tracked file:

```diff
 nros-rmw = { path = "../../../packages/core/nros-rmw" }  # nros-managed
-nros-zephyr-build = { path = "../../../packages/tooling/nros-zephyr-build" }  # nros-managed
 std_msgs = { path = "generated/std_msgs" }  # nros-managed
```

`examples/workspaces/rust` genuinely depends on that crate —
`src/zephyr_entry/Cargo.toml` and `src/zephyr_entry_robot1/Cargo.toml` both carry
it as a `[build-dependencies]` row. So the emitted table was wrong, not merely
narrower.

**Root cause: the `nros` binary was stale.** The CLI carries a hardcoded
crate → path table (`nros_crate_path_lookup`, `ws.rs:1987`). phase-321 W2.e moved
the build helpers out of `packages/core/` into `packages/tooling/`, and the
CURRENT source is correct:

```rust
("nros-zephyr-build", "packages/tooling/nros-zephyr-build"),
```

The compiled binary predated that commit, so it looked for
`packages/core/nros-zephyr-build`, found nothing, and **dropped the entry
silently**. Rebuilding the CLI fixed it: `nros sync` now exits 0 and leaves the
config byte-identical.

Rebuilding also cleared both failures originally filed here — the
`nros-launch-resolve` `--bringup-root` skew (that helper is built by a separate
recipe, `just setup-launch-resolve`) and the `metadata-mode harness failed
(exit 101)` at `metadata_build.rs:327`. Neither was an independent bug.

## The actual defect: the guard is not on the path people are told to use

A staleness guard **exists, and is good**. `scripts/build/cargo.sh:149` walks
`git ls-files packages/cli` for any `.rs`/`Cargo.toml`/`Cargo.lock` newer than
the binary and refuses with:

> `[ERROR] in-tree nros CLI is STALE … A stale CLI silently breaks workspace
> planning + codegen (issue #197 …)`

It even names this exact failure mode. But it lives in `nros_cli_bin()`, so it
only runs for callers that go through `just`. `activate.sh:65` puts the raw
binary on `PATH`:

```sh
export PATH="$_nros_root/packages/cli/target/release:$PATH"
```

so a bare `nros sync` — **the command CLAUDE.md and `nros-patch.toml`'s own
header both tell you to run** — never reaches the guard. The documented recovery
procedure is precisely the invocation the protection does not cover.

That is issue 0354's shape again (a validator whose callers do not include the
case it exists for), with a worse payload: not a missed check, but a silently
wrong `[patch.crates-io]` table. A dropped patch entry does not fail — the
dependency resolves from crates.io instead of the local checkout, which is
issue 0359's own thesis about artifacts that look authoritative and are not
consulted.

## Why it surfaced now

Every checkout whose `nros` binary predates phase-321's package moves emits this.
It was found because `check-leaf-lockfiles` went red on a stale central
`nros-patch.toml` (same move, same cause), and the documented fix — `nros sync` —
was run directly.

## Ways to fix

**A. Make the CLI refuse to drop a crate it cannot locate.** `nros_crate_path_lookup`
currently maps a name to a path and silently omits the entry when the path is
absent. Erroring instead — *"nros-zephyr-build: packages/core/nros-zephyr-build
does not exist; the lookup table is stale"* — converts silent corruption into a
loud failure, and works even when the binary is current but the table is wrong.
**This is the one that matters**: it is the only option that fails safe
regardless of why the path is bad.

**B. Put the guard on the direct path.** Either `activate.sh` exports a wrapper
that runs the staleness check and `exec`s the real binary, or the CLI self-checks
at startup (it can resolve `NROS_REPO_DIR` and compare its own mtime against
`packages/cli` sources). B closes the staleness case specifically; A closes the
class.

**C. Couple the CLI and its helpers.** `just setup-cli` should rebuild
`nros-launch-resolve` too, or fail when it is older — they are built by separate
recipes and must agree on an argument list, with nothing gating the pair.

**Recommended: A, then B.** C is worth doing alongside; it is a smaller
independent trap.

## A LANDED (2026-07-31)

`render_managed_entries` now refuses to emit a patch table that omits a managed
crate whose lookup path is dead, instead of silently `continue`-ing:

```
ws sync: ERROR — managed crate `nros-zephyr-build` maps to
  `packages/core/nros-zephyr-build`, which does not exist under /…/nano-ros.
Error: ws sync: 1 managed crate(s) have a dead path in the nros lookup table:
  nros-zephyr-build -> packages/core/nros-zephyr-build
Refusing to write an incomplete [patch.crates-io] — a missing entry resolves that
dependency from crates.io instead of this checkout, which fails nowhere.
Rebuild the CLI (`just setup-cli`); if that does not help, nros_crate_path_lookup
is stale.
```

Checked before making it fatal: **all 23 lookup-table paths are in-repo
directories with TRACKED `Cargo.toml`s**, so a dead path can only mean staleness
— never an unprovisioned submodule or an optional crate. Without that, erroring
would have broken sync on partial checkouts.

Returns a proper `eyre` error rather than panicking (one caller, small ripple),
collects every dead mapping so one run reports them all, and mutation-tested:
pointing the table back at `packages/core/nros-zephyr-build` reproduces the
original silent drop as a loud failure; restoring it leaves the config
byte-identical. CLI suite green (64 test binaries).

**Still open: B and C.** The staleness guard remains unreachable from a direct
`nros` invocation, which is what the docs tell people to run — so a stale binary
can still be wrong in ways the lookup table cannot catch. A closes the corruption
CLASS; B closes the staleness CAUSE.

## Not a fix

Atomicity (the original recommendation). The writes are already atomic, and the
failure mode is a complete write of wrong content.

## B AND C LANDED (2026-07-31)

**B — the guard moved onto the path people are actually told to use.**
`packages/cli/nros-cli-core/src/stale_guard.rs`: the binary checks ITSELF before
dispatch, so invocation style can no longer bypass it. Same predicate as the
shell guard (`git ls-files packages/cli`, `.rs`/`Cargo.toml`/`Cargo.lock`,
excluding `third-party/` and `testing_workspaces/`), so the two cannot disagree
about what "stale" means.

Three deliberate limits:

- **Only a binary inside `<root>/packages/cli/target/`.** An installed
  `~/.nros/bin/nros` is not stale *relative to* a checkout it does not belong
  to; blocking it would break every out-of-tree user.
- **Only commands that consume the crate→path table or emit artifacts** —
  sync, plan, ws, codegen, codegen-system, generate-rust, setup. `doctor`,
  `--version` and `completions` still run stale ON PURPOSE: diagnosing a broken
  checkout is exactly when you have one.
- The allow-list matches the **enum variant**, not a user-typed string, so a
  renamed verb cannot silently fall out of the guarded set.

`NROS_SKIP_STALE_CHECK=1` overrides for a deliberate experiment.

Verified: with a source touched newer than the binary, a bare `nros sync` — the
exact invocation that previously bypassed everything — now refuses and names the
offending file; `nros doctor` and `nros --version` still run; the env override
works; and after `just setup-cli` the guard goes quiet.

**C — setup-cli now notices when the resolver falls behind.** It warns (not
fails) when `nros-launch-resolve` is older than the CLI it just built. Warn
rather than fail because setup-cli's job is to produce the binary, and the
resolver has legitimate skip conditions (submodule absent, no CPython) that
would otherwise block a valid CLI-only setup.

It fired immediately on this checkout — the resolver WAS older — and went quiet
after `just setup-launch-resolve`.

