---
id: 1464
title: "`check-abi-bindings` skips in every CI lane — no image installs
  bindgen-cli, so nothing has ever verified the committed bindgen output"
status: open
type: bug
area: [ci, tooling]
severity: medium
found: 2026-09-23
related: [1226, 1040, 1043]
---

## What happens

Every scheduled tier-2 run prints

```
[SKIPPED] abi-bindings: bindgen-cli not installed (cargo install bindgen-cli --locked --version 0.72.1)
```

The skip itself is correct in SHAPE: `just/check/abi.just` goes through
`nros_check_skip`, so this is issue 1043's third outcome — NOT VERIFIED, with
a named remedy — rather than a green that means nothing. A tier-2 runner
without bindgen is a host fact, and a reported skip is the honest answer.

The problem is that no other lane answers it either.

## What was measured

`bindgen` appears in neither CI image:

```
$ grep -rn bindgen ci/docker/ci-base/Dockerfile ci/docker/zephyr-ros/Dockerfile
(nothing)
```

`ci-base` (`ghcr.io/newslabntu/nano-ros-ci:humble`) is the `container:` for
gate.yml's `check` job, which is where `check-fast` — and therefore
`check-abi-bindings` — runs on the merge-gating events. The only `bindgen`
matches in `.github/workflows/` are `libclang` installs for `zephyr-sys`'s
`build.rs`, which is a different tool doing a different job.

So the gate runs nowhere that has what it needs, in any lane, on any event.

## Why it matters

CLAUDE.md's rule: the C headers are the SSoT and Rust consumes COMMITTED
bindgen output (`packages/rmw/cffi/`, `packages/platform/nros-platform-cffi/`,
`packages/boards/nros-board-cffi/`). `check-abi-bindings` is the only thing
that notices a header edit landing without `scripts/gen-abi-bindings.sh`. A
contributor with bindgen installed locally is what stands between that and a
silent mismatch — and the gate reads as coverage to everyone else.

This is issue 1226's shape a lane over: a gate that WORKS is not a gate that
RUNS. 1040 (`check-default-gates-run-somewhere`) asks whether a gate is NAMED
by a lane; it cannot ask whether the lane's host can execute it, and a
`nros_check_skip` is indistinguishable from a pass in an aggregate verdict.

## Not this

* **Not a bad skip.** Do not make the gate fail-closed on a host without
  bindgen: that reddens every tier-2 run for a reason that is not about the
  bindings, which is the signal loss issue 1158 already records.
* **Not tier 2's problem.** Tier 2 is where it was noticed. `check-fast` in
  the gate lane is where it should be answered.

## What would close it

Either `cargo install bindgen-cli --locked --version 0.72.1` in `ci-base` (and
the `-rN` tag revision it implies), or a rule that a `nros_check_skip` must
name at least one lane whose host provides the missing tool — so a gate nobody
can run is a failure at the ledger rather than a green line in every log.
Prefer the first; the second is the general fix and is a bigger piece of work.

Measured 2026-09-23 from run-matrix 35826999550 and the two Dockerfiles at
`fcba471be`.
