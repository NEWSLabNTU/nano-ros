---
id: 426
title: "The source-metadata host probe compiles target-only deps and fails unhandled on Cortex-M node pkgs"
status: resolved
type: bug
area: build
related: [phase-307, issue-0413]
resolved_in: "phase-337 session"
---

## Symptom

`just qemu build-fixtures` prints a full rustc error mid-run and then **exits
0**:

```text
  → (node-pkg codegen) examples/qemu-arm-baremetal/rust/talker-rtic
  …
error[E0432]: unresolved imports `cortex_m::register::basepri`, `cortex_m::register::basepri_max`
  --> …/rtic-2.3.0/src/export/cortex_basepri.rs:2:26
note: found an item that was configured out
  --> …/cortex-m-0.7.8/src/register/mod.rs:34:9
   |
33 | #[cfg(any(armv7m, armv8m_main))]
```

The lane is not blocked — the fixtures do build — which is what makes this worth
filing rather than shrugging at: the error is LOUD and the consequence is
SILENT.

## Root cause

`nros sync`'s source-metadata refresh compiles a **host** probe per Node pkg to
extract declared entities. An RTIC / bare-metal Cortex-M node package's deps are
target-gated: `cortex-m 0.7.8` only emits its `armv7m` cfg for a `thumbv*`
target, and `rtic`'s `thumbv7-backend` unconditionally imports `basepri` behind
that cfg. Compiled for the host, the cfg is absent and the import fails.

The refresh already KNOWS this class and handles two instances of it explicitly
— the messages are in the same run:

```text
sync: source metadata — no producer for nuttx_talker::talker
  (cargo config sets `[unstable] build-std` for target `armv7a-nuttx-eabihf`;
   build-std is not target-scoped, so the host probe would rebuild std against
   that target's patched sysroot deps and fail to compile)
```

`thumbv7m-none-eabi` sets no `build-std`, so it does not match that guard, falls
into the unhandled path, and the probe failure surfaces as raw rustc output.

## Consequence

Two, both quiet:

1. Every RTIC / bare-metal Cortex-M node package silently gets NO source
   metadata sidecar, so bakes fall back to the SystemModel's entity lower
   bound rather than the measured set.
2. A real compile error in one of those packages is indistinguishable from this
   expected one, because both print the same way and neither fails the run.

## Fix (landed)

NOT by widening `probe_blocker` to "any non-host `[build] target`", which the
filing sketched. That would skip every Cortex-M leaf up front, including the
ones that probe FINE today (a plain `no_std` talker with no target-gated deps —
`examples/qemu-arm-baremetal/rust/talker-rtic` has a live sidecar). Trading a
noisy-but-correct probe for a silent skip is the wrong direction.

The actual defect is narrower and is in `metadata_build.rs`: the probe pipes
stderr and then echoes it **unconditionally**. The degradation machinery already
worked — a deploy-bound component that fails is negative-cached and reported via
`sync: source metadata — no producer for …`. So the failure was already handled;
what leaked was the raw output, printed BEFORE the handling and with nothing
tying the two together.

So the echo now happens only when the probe SUCCEEDS (where it carries the
harness's own diagnostics), and on failure the first rustc diagnostic is folded
into the error, which the caller already prints on the degradation line:

```text
sync: source metadata — no producer for qemu_rtic_main_e2e::rtic_run_plan_e2e
  (deploy-bound probe failed: metadata-mode harness failed (exit 101) for
   component 'rtic_run_plan_e2e': error[E0432]: unresolved imports
   `cortex_m::register::basepri`, `cortex_m::register::basepri_max`)
```

One line, names the package, the consequence and the cause. A REAL compile error
in such a package now reads the same way — which is correct, because it IS the
same event, and the operator gets the diagnostic either way.

`just qemu build-fixtures` no longer prints any `E0432` / `basepri` output and
still exits 0.

## Reproduce

```sh
just setup-cli && just qemu build-fixtures     # prints E0432, exits 0
```

Confirmed on `c3fcdd7bf`.
