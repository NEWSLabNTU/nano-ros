---
id: 1510
title: "One CLI, two repo-root ladders, opposite precedence — `abi_guard` puts the
  tree above the consumer FIRST and writes down why, and the planner puts
  ambient `NROS_REPO_DIR` first, so a direct `nros` in a worktree resolves the
  parent checkout"
status: resolved
type: bug
area: [cli, build, tooling]
severity: medium
found: 2026-09-27
related: [1280, 1391, 1336, 0951, 0196]
---

## What is wrong

The `nros` CLI answers "which nano-ros checkout is this?" in **two places, with
opposite precedence**, and one of them has the other's reasoning written in its
own doc comment.

`packages/cli/nros-cli-core/src/abi_guard.rs`, order most-specific first:

> 1. A nano-ros tree **ABOVE the consumer**. … **It beats the environment on
>    purpose: `NROS_REPO_DIR` is ambient after `source activate.sh`, so a
>    contributor with two checkouts open would otherwise have every consumer in
>    the second one measured against the first.**
> 2. `NROS_REPO_DIR` — the consumer said which checkout it links, and is not
>    inside one. Honoured as given, so a wrong value fails at the parse rather
>    than being silently ignored.

`packages/cli/nros-cli-core/src/orchestration/planner.rs` (and `nros build`,
which the comment says must agree with it) ladders the **other way**:

> `--nano-ros-path` → `NROS_REPO_DIR` → an autodetect walk → this toolchain's
> own `share/nano-ros`

So the ambient variable outranks the tree the consumer sits inside — exactly the
arm `abi_guard` documents as wrong, for exactly the reason it gives.

## The symptom, measured

An agent session working in a linked worktree ran `nros sync` directly (not
through `just`) and it **wrote patch paths into the PARENT checkout**, so the
first build compiled the main checkout's crates. Every measurement taken before
that was noticed described the wrong tree. Reported during phase-457 W1.

## Why the existing rules do not cover it

This is issue 1280's family and 1280 **already named this variable** in its own
evidence:

> an inherited `NROS_REPO_DIR` sent four `check::build` gates' fixtures into the
> main checkout's `build/`, so those gates ran and measured the wrong tree

and yet:

* **`NROS_REPO_DIR` is not in `scripts/lib/checkout-paths.sh`.** `git grep -n
  NROS_REPO_DIR scripts/lib/checkout-paths.sh scripts/lib/reroot-checkout-path.sh`
  is empty, so the re-rooting rule never sees it.
* **`just` is fine, by a different mechanism:** `just/sdk-env.just` has
  `export NROS_REPO_DIR := _NROS_HERE`, so every `just` road SETS it to this
  checkout rather than re-rooting an inherited one.
* **1280's gate drives three roads** — "the shell helper, `nros_build_root` AND
  `just` itself". A **direct `nros` invocation** is a fourth, and none of the
  three reaches it. That is issue 0196's shape: a reach narrower than the rule.

## What this is NOT

* Not "a variable needs adding to a list". The two ladders disagree about
  PRECEDENCE, and adding the variable to the shell rule would fix neither, since
  a direct `nros` does not go through it.
* Not a defect in `abi_guard`. Its order is the considered one and its comment is
  the best statement of the problem in the tree.
* Not covered by `just`'s re-rooting. `just` never gets the chance.

## Options

Each is a real choice with a cost; they are not variations of one fix.

**A — adopt `abi_guard`'s precedence in the planner.** Put "a nano-ros tree above
the consumer" ahead of `NROS_REPO_DIR`.
*For:* it is the same question, already argued, with the worktree case as the
stated motivation; two ladders in one binary disagreeing is the defect.
*Against:* it changes behaviour for anyone who sets `NROS_REPO_DIR` deliberately
*while inside* another checkout — which `abi_guard` already rules the wrong
answer, so the cost is narrow but real. A consumer that legitimately links a
different checkout must then pass `--nano-ros-path`, which still outranks both.

**B — re-root the value, keeping the order (1280's rule literally).** Honour
`NROS_REPO_DIR` first, but if it names a checkout that is *not* the consumer's,
re-root to this one through `nros_build_paths::reroot_foreign`.
*For:* preserves env-first where 1280 says it earns its keep — a path outside any
checkout is a real out-of-tree SDK and stays untouched.
*Against:* a THIRD spelling of one question, and the marker walk lives in a shell
helper the CLI does not share. Issue 1391 is what a second spelling of this rule
already cost.

**C — refuse on disagreement.** If the consumer is inside a checkout and
`NROS_REPO_DIR` names a different one, fail naming both paths.
*For:* matches `abi_guard`'s own stance ("a wrong value fails at the parse rather
than being silently ignored") and the repo's fail-loud preference; the ambiguity
is genuinely the operator's to resolve.
*Against:* breaks any legitimate cross-checkout workflow outright, and nobody has
measured whether one exists. Strictly worse than A if such a workflow does.

**D — one ladder, shared, then choose once.** Extract the precedence into a
single function both `abi_guard` and the planner call, and make A/B/C a single
decision in one place.
*For:* it is the repo's own "ONE shared helper rather than a second spelling"
rule, and the reason this bug exists at all. Also the only option that stops the
two from drifting again.
*Against:* the two callers do not want identical fallbacks — `abi_guard` has a
"tree above THIS BINARY" optimistic arm that `nros build` should not take, so the
shared function has to be parameterised rather than uniform.

**Recommended: D with A's precedence** — one ladder, the tree above the consumer
first, `NROS_REPO_DIR` second, the per-caller tail arms passed in. That is one
answer to one question, and it is the answer `abi_guard` already reasoned to.

## Acceptance

A direct `nros sync` run inside a linked worktree with an inherited
`NROS_REPO_DIR` naming the parent checkout must resolve the WORKTREE, with a
reproduction that fails first. Extend 1280's gate to the fourth road — a direct
CLI invocation — so the reach matches the rule.


---

# RESOLVED — option A, and the recommendation this issue shipped with was WRONG

## What landed

Two lines in `orchestration::nano_ros_root::resolve_from`: the workspace walk-up
now outranks `$NROS_REPO_DIR`.

```rust
 rungs.explicit
-  .or(rungs.repo_dir)
-  .or_else(|| rungs.workspace.and_then(autodetect_nano_ros_path))
+  .or_else(|| rungs.workspace.and_then(autodetect_nano_ros_path))
+  .or(rungs.repo_dir)
   .or_else(|| rungs.exe.as_deref().and_then(shipped_beside))
```

`explicit` (`--nano-ros-path` / `-DNANO_ROS_ROOT`) is untouched at rung 1 and is
now the only way to aim a build at another checkout — which is the right shape,
because it distinguishes INTENT from INHERITANCE and an exported variable cannot.

## The recommendation above is wrong, and reading the code is what showed it

This issue recommended **D — "one shared ladder, decide once"**, on the reading
that two ladders were two copies of one thing. They are not:

* `abi_guard::runtime_root(start)` answers **"which nano-ros tree does this
  CONSUMER link?"** — for the ABI / codegen-version guard. Rungs: tree above the
  consumer, `$NROS_REPO_DIR`, tree above THIS BINARY.
* `nano_ros_root::resolve_from(rungs)` answers **"where is the SDK root a BUILD
  reads from?"** — `cmake/`, `config/`, `packages/`. Rungs: explicit,
  `$NROS_REPO_DIR`, a walk from the workspace, `<prefix>/share/nano-ros`.

Different questions, different rung SETS, and the second is **already the
consolidation D was asking for**: phase-447 A2 moved it here out of five
hand-written copies, and its header says so — *"One ladder, spelled once … Five
call sites carried the three-rung chain by hand."* Eight sites ask it today.

So D would have undone a deliberate consolidation and forced a parameterised
function over two concepts. The real defect was never duplication; it was **one
rung of precedence**, and A fixes exactly that while leaving both ladders whole.

The other two options stay rejected for the reasons given above, and one gets
worse on inspection: **C (refuse on disagreement)** punishes the normal setup,
because every contributor has `$NROS_REPO_DIR` from `activate.sh` and agent
sessions work in worktrees by default — the common case would become a hard
error demanding a flag.

## Verified, with a real negative control

`the_workspace_checkout_outranks_an_inherited_repo_dir` builds two checkouts with
the child NESTED inside the parent exactly as an agent worktree is
(`<main>/.claude/worktrees/<id>` — the shape that makes a lexical prefix test
useless, issue 1391), and asserts the walk wins.

On the OLD rung order it fails with the right diagnosis:

```
left:  Some(".../parent")
right: Some(".../parent/.claude/worktrees/agent-x")
```

**The first version of that test was broken and "failed" for the wrong reason** —
`MONOREPO_MARKER` is `packages/core/nros-core/Cargo.toml`, a nested path, so
writing it without creating the directory raised `NotFound`. A failing test is
not a reproduction until you have read WHY it failed; the fixture was corrected
and the control re-run before the swap was trusted.

`resolve_from` takes its rungs as a struct, so none of this touches the
filesystem beyond the two fixtures — the reproduction is a unit test, not a
build.

Also green: `nros-cli-core` 1,436 lib tests, clippy `-D warnings`, `check fast`
363 gates.

## What did NOT change

`abi_guard::runtime_root` is untouched. Its order was already right and its
comment is still the best statement of the hazard in the tree — it is what this
fix was argued from.
