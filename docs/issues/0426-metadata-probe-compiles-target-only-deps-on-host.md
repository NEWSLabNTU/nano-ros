---
id: 426
title: "The source-metadata host probe compiles target-only deps and fails unhandled on Cortex-M node pkgs"
status: open
type: bug
area: build
related: [phase-307, issue-0413]
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

## Fix direction

Same shape as the `build-std` guard: decide up-front whether a package is
host-probeable and SAY SO, instead of attempting the probe and letting rustc
explain. The signal is available without compiling — the leaf's
`.cargo/config.toml` names a `[build] target`, and a cross target that is not
the host means the probe cannot run.

Widen the existing "no producer for X" branch from "sets build-std" to "declares
a non-host `[build] target`", which subsumes the two current cases and this one,
and keep the reason string per-cause so the message still says WHY.

## Reproduce

```sh
just setup-cli && just qemu build-fixtures     # prints E0432, exits 0
```

Confirmed on `c3fcdd7bf`.
