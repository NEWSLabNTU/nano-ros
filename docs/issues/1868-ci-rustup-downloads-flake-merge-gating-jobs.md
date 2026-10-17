---
id: 1868
title: "Merge-gating CI jobs fetch Rust toolchains and targets from static.rust-lang.org at run time, and a failed download ejects the batch"
status: open
type: bug
area: [ci]
severity: medium
found: 2026-10-13
related: [1353, phase-505]
---

**Home: [phase-505](../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## Measured

Three times in under four hours on 2026-10-13, a rustup download failure, not
the code, produced a red `CI`:

| When (UTC) | Run / job | PR | Decisive line |
| --- | --- | --- | --- |
| 01:07 | merge_group gate 38329139207, `check / test-lane-contracts` | #2094 ejected | `error: could not download file from 'https://static.rust-lang.org/dist/channel-rust-1.99.0.toml.sha256'` (in `setup-cli`) |
| 03:3x | PR gate 38340687228, job 115110810172 `check / fast` (`leaf-lockfiles`) | #2111 red | `could not download file from 'https://static.rust-lang.org/dist/channel-rust-1.93.0.toml.sha256'` |
| 04:4x | merge_group gate 38347134207, `check / workspace-all` (`workspace-embedded`) | #2114 ejected | `component download failed for rust-std-armv8r-none-eabihf: could not rename 'downloaded' file …` |

Each was cleared by a re-run or a re-arm with no change, and each PR was
docs-only or unrelated to the failing step. An ejection also ejects every queue
entry behind the failing one, so one bad download costs up to a batch of five.

## What it is, and what it is not

The jobs install toolchains and targets on demand: `rust-toolchain.toml`
channels, `+nightly`, and the embedded `rust-std` components that
`workspace-embedded` needs. Each install is a network fetch inside a
merge-gating job, so the gate's verdict depends on static.rust-lang.org being
reachable at that moment. There are two channels in play: 1.99.0 (the root
toolchain) and 1.93.0 (a leaf pinned older, reached by `leaf-lockfiles`).

It is NOT 1353 (disk). It is NOT a defect in any of the three PRs.

## What would close it

Either of these, measured by a week with no rustup-download reds:

1. Bake every channel and target the merge-gating jobs install into the CI
   image (`ci-base`), the way bindgen was (issue 1464).
2. Or restore them from `actions/cache` keyed on the toolchain files, with a
   bounded retry around `rustup toolchain install` / `rustup target add`.

A retry alone hides the dependency rather than removing it.

## Update 2026-10-13 05:20: two shapes, and one is a race, not the network

Two more merge-queue ejections followed: #2115 (merge_group gate 38348211661)
and #2114 again (38349553423). Both were in `check / workspace-all`:

```
error: component download failed for rust-std-armv7a-none-eabi: could not rename 'downloaded' file from '/usr/local/rustup/downloads/….partial' to '…'
error: component download failed for rust-std-aarch64-unknown-none: could not rename 'downloaded' file …
```

Together with #2114's first ejection, that makes three `could not rename`
failures in that one job, each on a different `rust-std` target. That is not a
network failure. `check::workspace-all` (`just/check/lanes.just`) forks
`check test-targets` and `check workspace-embedded` with `&`. Each half's first
`cargo` goes through the rustup proxy, which installs the targets the root
`rust-toolchain.toml` names when they are missing. Two proxies installing at
once race on `/usr/local/rustup/downloads`. The same file already says so for
`rustup target add` ("FIRST, serially: two concurrent adds of different
targets touch the same toolchain dir"). `workspace-all` is the one fork that
did not follow it; a sweep of `&`-forked cargo/just calls finds no other.

Fix, in the same change: `workspace-all` runs `cargo --version` once before
forking, so the toolchain is complete before either half starts.

The OTHER shape, `could not download file from 'https://static.rust-lang.org/…'`
(#2094 at 01:07 and #2111), is a real fetch failure. The image or cache
remedy above still applies to it.

## The same race in `check fast`'s own fan-out (2026-10-16)

Two of four push gates on main failed in `check / fast (every event)`, in
the `cbindgen-headers` gate, while rustup installed missing components of the
pinned 1.99.0:

- Gate run **38866908749** (`9338261f4`), job **116861795238**:
  `error: component download failed for rust-std-riscv32imc-unknown-none-elf: could not rename 'downloaded' file`
- Gate run **38871191298** (`d49b65dbf`), job **116875496284**:
  `error: failed to install component: 'clippy-preview-x86_64-unknown-linux-gnu', detected conflict: 'bin/cargo-clippy'`

The runs between them passed. `check fast` runs its gates through
`scripts/build/run-gates-parallel.sh`, which forks them with `xargs -P`. The
2026-10-13 sweep looked for `&`-forked cargo and `just` calls, so it missed
this fan-out. `xargs -P` in that script is the only one in `scripts/`,
`just/` and `.github/`.

Fix, in the same change: the runner runs `cargo --version` once before the
fan-out, the same line `workspace-all` got.
