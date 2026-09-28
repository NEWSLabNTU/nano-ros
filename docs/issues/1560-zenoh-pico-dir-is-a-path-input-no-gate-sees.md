---
id: 1560
title: "Four path-valued build inputs resolve without the re-root rule, each
  outside a different edge of the gate's subject"
status: open
type: bug
area: build
severity: medium
found: 2026-09-29
related: [0196, 0491, 1280, 1336, 1527, 1558, phase-471, RFC-0101]
---

## What this is

RFC-0101 D3: a path-valued build input is resolved through `nros_build_paths`,
which is the one implementation of issue 1280's three-valued rule (a value
outside any checkout is kept, a value naming a DIFFERENT checkout is re-rooted
onto this one, this checkout's own is kept). Issue 1527 swept the build scripts
and phase-471 W3 added the gate.

Four inputs are still resolved with a bare `std::env::var`, and **no two of them
are outside the gate for the same reason.** That is the finding: this is not one
missed site, it is four different mismatches between the rule (*a path-valued
build input*) and the gate's subject.

## The four, and why each is invisible

| # | site | variable(s) | why the gate cannot see it |
| --- | --- | --- | --- |
| 1 | `packages/boards/nros-board-common/src/threadx_qemu_riscv64_build.rs:339` | `THREADX_DIR`, `NETX_DIR`, `NROS_VIRTIO_NET_NETX_DIR` | it is **not a `build.rs`**. The census and the gate read `build.rs` files; this is a shared build-script library. It is also a private helper taking the name as an ARGUMENT — issue 1527's own hiding mechanism |
| 2 | `packages/tooling/nros-platform-config/src/manifest.rs:605` | the nine `{env:VAR}` tokens in three `nros-platform.toml` descriptors | it is **not Rust that names a variable at all** — the variables are data in a TOML descriptor, interpolated generically. A grep over build scripts finds none of them |
| 3 | `packages/rmw/zenoh/nros-zpico-build/src/runner.rs:2581` | `ZENOH_PICO_DIR` | the variable has **no `just/sdk-env.just` row**, and the gate reads its variable set from that file so the shell and Rust halves cannot drift apart |
| 4 | `packages/drivers/ipc/nvidia-ivc/build.rs:26` | `NV_SPE_FSP_DIR` | same as 3 — no `sdk-env.just` row, because the FSP ships under an SDK-Manager EULA and can never be vendored, so there is no in-repo default to export |

Site 1 is the live one. Its twin, `env_path_or` in
`packages/boards/nros-board-threadx-linux/build.rs:176`, was converted by issue
1527 to delegate to `nros_build_paths::env_path`, and its doc comment says why:

> taking the variable name as an ARGUMENT is what kept that invisible: no
> literal-matching probe can tell which variables a helper like this resolves.

The unconverted copy one file away is `env::var(name).unwrap_or(default)` then
`canonical` — canonicalisation without the re-root, which is the half that does
not matter. So a RISC-V ThreadX build in an agent worktree that inherited
`THREADX_DIR` from a parent shell compiles the **other checkout's** kernel,
which is issue 1280 exactly, in the family issue 1527 was opened to close.

Sites 3 and 4 are narrower than they look and should be said so rather than
inflated: both name a path that in practice lies outside every nano-ros
checkout (a user's install prefix; an EULA'd SDK), which is the case
`reroot_foreign` deliberately leaves alone. They are filed because D3 is a rule
about the CALL, not about which value happens to arrive — a rule with a "when it
would not have mattered anyway" arm cannot be checked.

Site 2 is the largest by reach and the least wrong in shape. The declarative
form is right (it is RFC-0049's platform rung, `required_env` validates
presence, and a row can be capability-gated in a way no `if` in a build script
reads as well — issue 1143). What it is missing is only the resolver, and the
precedent sits in the same `match`: the `{nuttx_include}` arm already routes
through `nros_build_paths::nuttx_include_root`, with a comment saying the shared
spelling is the point.

## Remedy

1. **Site 1** — make `threadx_qemu_riscv64_build::env_path_or` delegate, exactly
   as its threadx-linux twin does:
   `nros_build_paths::env_path(name).unwrap_or_else(|| nros_build_paths::canonical(&default))`.
   *Do this one first; it is the only one with a reachable wrong answer.*
2. **Site 2** — route `manifest.rs`'s `env:` arm through
   `nros_build_paths::env_path`, keeping the `InterpError::MissingEnv` behaviour
   for an unset variable. Note this changes nothing for a `just`-driven build,
   where `sdk-env.just` has already re-rooted the value; it changes the answer
   for a bare `cargo` in a worktree, which is the 1280 scenario.
3. **Sites 3 and 4** — `nros_build_paths::env_path`, keeping each panic message.
4. **The gate.** Whichever of the two directions is chosen, say which:
   * widen the gate's SUBJECT from "a `build.rs`" to "a build-script input",
     which means reading shared build-script libraries and the descriptor
     interpolator, and from "a variable in `sdk-env.just`" to "a path-valued
     name"; **or**
   * require every path-valued build input to have an `sdk-env.just` row, which
     keeps the subject small and is what D4 already asks for — but has no
     answer for `NV_SPE_FSP_DIR`, which has no in-repo default to export.

   The two answers differ only on site 4, which is why it is worth deciding
   rather than drifting into whichever the next edit implies.

## Why this is 0196's shape, and running the other way too

Issue 0196's rule is that a gate's reach must match the rule it enforces. Sites
1 and 2 are the classic direction — reach narrower than the rule, so it reports
green about code it never read. Sites 3 and 4 are the direction issue 1452
found: the gate's subject is a hand-authored list (`sdk-env.just`'s rows), so it
is only as complete as whoever wrote it, and rooting a check in an authored
field re-creates one level up the problem it exists to answer.

## Acceptance

`just check fast` green, and a measurement rather than a claim for site 1: with
`THREADX_DIR` exported to a DIFFERENT checkout, the ThreadX RISC-V build
compiles this checkout's kernel. Before the fix it compiles the other one.

## Related

* RFC-0101 D3 / D4 / D5 — the rule, and the descriptor road as a first-class
  carrier of a source root.
* Issue 1558 — the other half of D3 (a repo root counted by `.parent()` hops).
* Issue 1527 — the sweep this is the residue of; phase-471 W6 holds the two
  NuttX sites it deliberately left open, which are not this issue.
