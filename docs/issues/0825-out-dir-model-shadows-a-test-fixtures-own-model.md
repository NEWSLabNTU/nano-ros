---
id: 825
title: "A stale model under `$OUT_DIR` outranks a bringup's own committed model,
  so one `nros` run poisons every later test using a bringup of the same NAME"
status: open
type: bug
area: build
related: [issue-0320, phase-330, phase-383]
---

## Problem

`model_search_paths` (`nros-orchestration-ir/src/model_location.rs:38`) resolves
a SystemModel in this order:

1. `$NROS_MODEL_DIR/<bringup>/<name>`
2. **`$OUT_DIR/nros/<bringup>/<name>`**
3. `<ws>/build/nros/models/<bringup>/<name>`
4. `<bringup>/build/nros/models/<bringup>/<name>`
5. `<bringup>/<model_rel>` — the committed copy

Rung 2 is keyed on the bringup's **directory name only**. `demo_bringup` is the
name nearly every workspace and nearly every test fixture uses. And `$OUT_DIR`
during `cargo test` is a **persistent, shared** build directory —
`packages/cli/target/debug/build/nros-cli-core-<hash>/out` — that survives
across runs and is shared by every test in the crate.

So a single `nros` invocation that resolves a model writes
`$OUT_DIR/nros/demo_bringup/system_model.yaml`, and from then on **every test
whose fixture builds a bringup called `demo_bringup` reads that file instead of
its own**, no matter where the fixture lives.

## What it looked like

Three `cmd::codegen_system` tests failed:

```
tier resolution: callback group `control_node/ctrl` names tier `high`,
which has no `[tiers.high]` definition
```

The fixture's `system.toml` plainly declares `[tiers.high]`, and instrumenting
confirmed the file on disk contained it (`file_has_tiers=true`) while the parsed
`SystemToml.tiers` was **empty**. `apply_model_execution`
(`model_ingest.rs:96`) overwrites `system.tiers` wholesale from the model, so a
model with no `execution.tiers` silently erases tiers the user authored.

The shadowing file resolved to:

```
packages/cli/target/debug/build/nros-cli-core-15f52a05d1e8b143/out/nros/demo_bringup/system_model.yaml
```

Zero occurrences of `tiers`, written by an unrelated `nros build` run hours
earlier. Deleting `.../out/nros` made all 21 tests pass.

## Why it was expensive to find

It reads as everything except what it is:

* **Host-specific.** Passes in a fresh worktree (no `$OUT_DIR` residue), fails
  in a long-lived checkout. That invites "works on my machine" and a bisect that
  converges on nothing.
* **Order- and cwd-sensitive in a way that is pure coincidence.** Whether it
  fires depends on whether anything previously wrote that path, so the same
  commit passes and fails depending on what ran before it — including from a
  different working directory.
* **It survives `cargo clean -p`.** The residue lives under a build-script `out`
  directory keyed by a crate hash; a targeted clean can miss it.
* **It is not a regression.** `origin/main` fails identically in an affected
  tree, which makes it look like someone else's problem, which makes it get
  skipped.

Ruled out on the way, each costing a cycle: the model cache under
`$XDG_CACHE_HOME`, the `nros-launch-resolve` binary's presence, submodule drift,
environment differences (only `PWD` differed), and stale incremental artifacts.

## Fix — directions, not yet chosen

The `$OUT_DIR` rung exists so a cargo build can hand its own build script a
freshly resolved model, which is legitimate. What is wrong is that the rung is
**keyed on a name that is not unique** and **read by consumers that never wrote
it**.

* **Namespace the rung by identity, not by name.** The bringup's absolute path
  (hashed) rather than its `file_name()`. `$XDG_CACHE_HOME` already does this —
  `<hash>-<bringup>` — and does not have the bug.
* **Or restrict the rung to the writer.** Only consult `$OUT_DIR` when this
  process's own build script produced the model, rather than whenever the
  variable happens to be set. A test binary inheriting `$OUT_DIR` is not the
  case the rung was written for.
* **Independently: `apply_model_execution` should not erase authored tiers with
  an empty model.** A model that declares no `execution.tiers` almost certainly
  means "this model does not speak about tiers", not "the system has none". The
  fail-loud rule in RFC-0052 argues for refusing, or for leaving the authored
  tiers in place, rather than a silent wholesale overwrite.

## Sweep

Every consumer of `model_search_paths`, and any other lookup keyed on a bringup
DIRECTORY NAME rather than its path:

```sh
grep -rn 'model_search_paths\|resolve_model_path\|file_name()' \
  --include='*.rs' packages/core/nros-orchestration-ir/src packages/cli
```

Immediate mitigation for anyone hitting it: `rm -rf
packages/cli/target/*/build/nros-cli-core-*/out/nros`.
