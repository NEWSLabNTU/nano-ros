---
id: 2654
title: "The `nros` CLI's release build is not incremental, so a one-line edit rebuilds for 35 s"
status: resolved
type: perf
area: [cli, build]
severity: medium
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "build(cli): the CLI's release build is incremental (issue 2654)"
related: [phase-489]
---

**Home: [phase-489](../roadmap/phase-489-ci-build-speedup.md)** (build speed). Measured 2026-11-16 on host 118 (48 cores, load 11–36 from other sessions), `packages/cli` at main `c49aaaaef0`, each build in its own target dir. Raw logs and `--timings` reports: `cli-profile/` in the measuring worktree.

## Measured

`just setup-cli` builds `cargo build --release` in `packages/cli`. That workspace
has no `[profile.release]`, so it gets cargo's default: `incremental = false`.

| build | wall | `nros-cli-core` |
| --- | --- | --- |
| release, edit one `nros-cli-core` source | **35.3 s** | 33.1 s (frontend 18.3, codegen 14.8) |
| release + `incremental = true`, same edit | **4.7 s** | 4.1 s (frontend 3.2, codegen 0.9) |
| release, cold | 48.1 s | 34.7 s |
| release + `incremental = true`, cold | 44.5 s | 31.7 s |

So incremental cuts an edit rebuild by 87 % and costs nothing cold. It also
cuts the every-commit rebuild of issue 2655 from 38.8 s to 5.0 s (two runs:
4.96, 5.25 s).

## Cost and risk

- One setting: `[profile.release] incremental = true` in `packages/cli/Cargo.toml`,
  or `setup-cli` passing `--config profile.release.incremental=true`.
- Disk: the target dir grows from 0.59 GB to 1.7 GB.
- sccache cannot cache an incremental crate, and the root `.cargo/config.toml`
  warns that `CARGO_INCREMENTAL=1` makes it abort. A profile setting is not the
  env var, but a CI job that wraps rustc in sccache should be checked, and may
  keep `incremental = false` there (CI builds cold anyway).
- The binary's runtime is unchanged: opt-level 3 is kept.

## Ruling (2026-11-16, user)

GO, with no added cleanup. Measured: the cache does not grow with use. Over six
edit-rebuild cycles the incremental target stayed at 1.7 GB, with 34 sessions
throughout, because rustc garbage-collects its own stale sessions. A long-lived
non-incremental CLI target is 763 MB, and `cargo sweep --time 14` and
`--installed` find nothing to clean in it. So the steady cost is about +1.1 GB
per checkout, bounded. (`cargo clean gc` covers cargo's global caches only,
not target dirs.)

Why incremental was off before: phase-340 W1 (b23df61484) dropped it from the
shared FIXTURE profiles. Those lanes build each leaf once, so the state was
written and never read, and dropping it saved 27.6 % of disk. Phase-340 W6
(78375a2d5d) turned it off on `dev`, for sccache's byte-identical rlibs.
Neither reason reaches `release`, which only host tools build.

## Where: A against B (user ruling 2026-11-16: B)

A shared `[profile.release]` in `.cargo/config.toml` must equal the root
manifest's under `check-cargo-profile-mirror`, and that one carries fat LTO,
`codegen-units = 1` and `panic = "abort"`. So the shared placement was not
available, and two options were measured (host 118, 48 cores, each in its own
target dir):

| | today | A: shared `nros-iterate` preset | B: release + `incremental = true` |
| --- | --- | --- | --- |
| cold | 48.1 s | 40.6 s | 44.5 s |
| edit one cli-core source | 35.3 s | 5.0 s | 4.7 s |
| every commit (index touch) | 38.8 s | 5.1 s | **5.0 s** |
| binary | 39.2 MB | 179 MB (`debug = 1`) | 39.4 MB |
| target dir | 0.59 GB | 2.3 GB | 1.7 GB |

A was refused for three reasons:
- `panic = "abort"` ends `nros setup`'s per-step `catch_unwind`
  (`cmd/setup/session.rs`), so one panicking install step would kill the
  whole run instead of being reported;
- 43 tracked files name `target/release/nros`;
- the shipped `nros` (`release-nros.yml`, `cargo build --release`) would be
  built differently from the dev-mode one ASI uses daily.

## Resolution

B: `incremental = true` in `packages/cli/Cargo.toml` `[profile.release]`,
with a comment there saying why it is not in the shared file. release-nros.yml
is left as it is, so the shipped binary builds with the identical profile (true
parity, no override). Incremental release raises rustc's default codegen units
from 16 to 256, so the binary is a different build, but with the same
opt-level, LTO setting and unwinding (39.4 against 39.2 MB).

Measured (raw `cargo build`): every-commit rebuild **38.8 s to 5.0 s**; edit
rebuild 35.3 s to 4.7 s.

Through `just setup-cli`, measured after the change:
- A pull that changes a CLI source: **36.6 s to 11.4 s** (two runs, 11.3 and
  11.4 s; the "before" is the same recipe with
  `CARGO_PROFILE_RELEASE_INCREMENTAL=false`). It costs more than raw cargo's
  4.7 s because the source stamp's VALUE changes, which re-bakes its `env!`
  constants (issue 2655).
- A commit that touches no CLI source: 0.87 s. `setup-cli` asks the binary's
  `source-stamp` first, finds it fresh, and never runs cargo.

So the 38.8 s every-commit rebuild was paid by DIRECT cargo invocations in
`packages/cli` (`bootstrap.sh`, the `cli-tests` gate, a contributor's `cargo
build`), not by `setup-cli`.
