---
id: 797
title: "Five nightly jobs abort in 2-5 s: the matrix builds only `--bin nros` and never provisions `nros-launch-resolve`"
status: resolved
type: bug
area: ci
related: [issue-0409, issue-0285, issue-0037]
---

## Symptom

Nightly has been red on every real run — 1377 s, 1322 s, 1634 s, 1681 s across
the last four (the 10-13 s "successes" between them are no-op invocations).
Five of eight jobs fail, and four of them fail almost immediately:

| job | fails at |
| --- | --- |
| `nuttx` | Test / e2e, **2 s** |
| `threadx_linux` | Build, **3 s** |
| `freertos` | Build, **4 s** |
| `esp32` | Build, **5 s** |
| `qemu` | Build, 120 s |

A 2-5 s failure is not a compile error. It is a precondition failing before work
starts, and it means **nightly currently provides almost no coverage** — the
matrix looks like it is exercising six platforms and is exercising one
(`threadx_riscv64`, which passes in 1272 s).

## Cause

From the `threadx_linux` job log:

```
Error: sync: 1 SystemModel(s) need resolving but `nros-launch-resolve` is not
       next to the `nros` binary
  threadx_linux_rs_action_client — build/nros/models/action-client/system_model.yaml is missing
Build it:  ./scripts/bootstrap.sh   (contributors: just setup-launch-resolve)
Refusing to continue with stale models — a museum SystemModel builds clean and
then places nodes wrong.
    nros-cli-core/src/cmd/ws.rs:1710
```

The nightly matrix provisions the CLI by hand rather than through the recipes:

```yaml
- name: Build nros CLI from packages/cli/ (Phase 218)
  run: |
    git submodule update --init --depth 1 packages/cli/third-party/play_launch
    cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
    echo "$GITHUB_WORKSPACE/packages/cli/target/release" >> "$GITHUB_PATH"
```

That builds **only `--bin nros`**. Nothing in the job ever runs
`just setup-launch-resolve`, so the resolver binary is never produced. The
submodule IS initialised, so this is not the 0409 "submodule missing" path — the
recipe that would build the resolver is simply never invoked.

`just <plat> setup` and the `play_launch_parser` provisioning step both run, and
neither builds the resolver; the parser (issue 0037) and the resolver (RFC-0060
layer 2) are different binaries.

## The refusal is correct

Worth stating so nobody "fixes" this by loosening the check. Issue 0409 records
that exiting 0 without a resolver let `nros sync` run against whatever stale
binary was on disk, and a resolver predating rlm v0.1.1 silently dropped every
`[[component]].params` projection — 22 models in `features/` lost their params
with no error. The hard failure here is that lesson working as intended. The bug
is the missing provisioning step, not the guard.

## Fix

Add the resolver to the nightly matrix's provisioning, next to the CLI build.
Two shapes, and the choice needs someone who can iterate on CI:

* `just setup-launch-resolve` after the `cargo build --bin nros` step. Minimal,
  and matches what contributors are told to run.
* Replace the hand-rolled `cargo build` with `just setup-cli` +
  `just setup-launch-resolve`, so CI provisions the same way everything else
  does and cannot drift again. Larger change; `setup-cli` does more than the
  current two lines.

**Unresolved detail, needs checking before either lands.** The error says the
resolver must be "next to the `nros` binary", but `setup-launch-resolve` builds
into `packages/cli/nros-launch-resolve/target/release/` while nightly's `nros`
lands in `packages/cli/target/release/`. On a dev box both exist in those
separate directories and `nros` finds the resolver anyway, so there is a search
path involved that this issue has not traced. Whoever fixes this should confirm
where CI needs the binary rather than assuming the recipe alone is sufficient —
otherwise the step runs, reports success, and the job still fails.

## Why this is worth doing before any nightly speed work

Nightly's cost profile has been measured (phase-371): container init ~500 s
across jobs, the CLI rebuilt per job ~420 s, `threadx_riscv64` 837 s of build.
All of that is real, and all of it is secondary — a red nightly that aborts in
seconds is not slow, it is absent.

## Resolved (2026-08-25) — `just setup-launch-resolve` in all FOUR nightly blocks

### The unresolved detail, answered

This issue said to confirm where CI needs the binary before fixing, because the
error says "next to the `nros` binary" while the recipe builds elsewhere —
"otherwise the step runs, reports success, and the job still fails".

Traced in `ws.rs::resolver_from`. The search has three arms and never uses
`$PATH`:

1. `$NROS_LAUNCH_RESOLVE`;
2. a SIBLING of the running `nros` — this is the arm the error message's wording
   describes, and the one that would send you to copy the binary;
3. `<repo>/packages/cli/nros-launch-resolve/target/release/`, reached via
   `dir.ancestors().nth(4)` from the running exe.

Nightly's `nros` lands in `packages/cli/target/release`, so `nth(4)` is the repo
root and arm 3 resolves to exactly where `setup-launch-resolve` writes. **No
copying, no `$NROS_LAUNCH_RESOLVE`, no layout change** — the recipe alone is
sufficient. Confirmed by running it: `nros` at nightly's path plus a real
`nros sync examples/workspaces/features` → "sync: done."

Also checked `CARGO_TARGET_DIR` is unset in the workflow; the recipe honours it
(issue 0400) and would otherwise write outside arm 3's path.

### FOUR blocks, not one

`grep -c` found the provisioning block **4 times** in `nightly.yml`. A fix to the
one quoted in this issue would have left three jobs failing exactly as before.

### The other two workflows are deliberately NOT touched

The same block appears once in `pr-checks.yml` and twice in `host-tests.yml`.
They are not patched, and that is a decision rather than an oversight: only
nightly is red. The refusal fires only when `nros sync` meets an unresolved
SystemModel, so those lanes evidently do not reach it — adding a ~30 s cargo
build to every PR to pre-empt a failure that is not occurring is a cost with no
evidence behind it. If one of them ever hits the same refusal, this section is
the note that it needs the same one-line step.
