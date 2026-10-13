---
id: 1868
title: "Merge-gating CI jobs fetch Rust toolchains and targets from static.rust-lang.org at run time, and a failed download ejects the batch"
status: open
type: bug
area: [ci]
severity: medium
found: 2026-10-13
related: [1353]
---

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
