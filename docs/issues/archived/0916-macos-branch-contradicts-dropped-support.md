---
id: 916
title: "Live macOS branches were added after macOS was dropped as a host — and
  the `native`/`posix` scaffold aliases behave differently"
status: resolved
type: bug
area: cli, tooling
related: [phase-401, phase-260, phase-327]
---

## Problem

Three findings from phase-401's naming audit that are **behavioural, not
naming**, and were therefore reported rather than changed on a vocabulary pass.

### 1. `detect_package_manager` special-cases macOS

`packages/cli/nros-cli-core/src/cmd/setup.rs:1193`:

```rust
/// … macOS is always `brew`.
pub(crate) fn detect_package_manager() -> Option<&'static str> {
    if std::env::consts::OS == "macos" {
        return Some("brew");
    }
```

and `setup.rs:2003-2041` asserts `macos-arm64` dispositions.

AGENTS.md has said the opposite since 2026-06-18 (phase-260):

> **Supported hosts: Linux (primary) and \*BSD (POSIX path). macOS is NOT
> supported** … Do not add `APPLE`/`target_os = "macos"`/`*-apple-darwin`
> branches to nano-ros source, CMake, or CI.

The branch was added by **phase-327**, i.e. *after* the prohibition. The same
crate contradicts itself: `orchestration/planner.rs:290` reads "macOS dropped
(phase-260) — no darwin/apple hosted target".

**Not obviously wrong on its own terms** — `nros setup --system` composes a
prerequisite install command, and knowing that macOS uses brew does not claim
nano-ros builds there. That is precisely why this needs a decision rather than
a patch: either the prohibition means what it says and the branch goes, or the
prohibition is narrower than its wording and should say so.

### 2. The `native` and `posix` scaffold aliases behave differently

`packages/cli/cargo-nano-ros/src/scaffold.rs:95-106` makes `native` and `posix`
aliases for one `PlatformKind::Hosted`, differing only in `deploy_token`. But
`scaffold.rs:1252` and `:1328` special-case `if platform != "native"` by
literal string for the C/C++ scaffolds, so **the `posix` alias writes a
`.cargo/config.toml` the `native` alias does not**.

Two spellings of one platform that produce different trees. Under phase-401's
rule `native` is the ROLE and `posix` the REACH, so they are not synonyms — but
they are not a behavioural axis either, and nothing documents this split.

### 3. A stale, self-contradicting comment in the scaffold journey check

`scripts/ci/scaffold-journey-check.sh:23` says `--platform native` "scaffolds
nros commented out (a stub), so it would not exercise the patch block", then
line 36 says `posix` **does** emit active deps. The Hosted template
(`scaffold.rs:893`) emits an **active** `nros` dep; the commented-out form was
retired at `scaffold.rs:1497`. Fixing the comment needs a re-check of whether
the `plat="baremetal"` choice it justifies is still necessary — a behavioural
question, not a wording one.

## Why these are filed together

All three are the same shape: a decision landed (macOS dropped; the scaffold
template stopped emitting a stub), and a site that encodes the old world was
left behind. None is a naming error, so phase-401's audit could not fix them
without exceeding its remit — but the audit is how they surfaced, and dropping
them on the floor would waste that.

## Suggested resolution

1. Decide whether phase-260's prohibition covers prerequisite tooling. If it
   does, delete the brew branch and the `macos-arm64` dispositions; if it does
   not, amend AGENTS.md so the exception is stated rather than inferred.
2. Make `native` and `posix` produce identical trees, or document why they must
   not — and if they must differ, the difference belongs on an explicit field,
   not on a string comparison against one of two aliases.
3. Re-check and correct the scaffold-journey comment.

## Resolved — all three, and one thing left standing on purpose

### 1. The brew branch is gone

Removed. The deciding evidence is that nano-ros cannot BUILD on macOS for two
independent reasons — `nros-platform-posix/src/timer.c` calls `timer_create`,
which macOS does not implement, and `nros-board-linux` calls
`sched_setaffinity`, absent from libc's apple module — so naming a package
manager there offered a first step down a road with no second one. A macOS host
now falls through to `None`, which callers already render as
`<no supported package manager detected>`.

That is the shape phase-260 W3 chose for the same problem in the planner: *"an
explicit unsupported-host error, not a silent route"*.

**AGENTS.md's acceptance widened to match.** It read "No `apple`/`darwin`/
`APPLE` branch", which is why `env::consts::OS == "macos"` survived the sweep —
the same claim in a spelling the criterion did not name. That spelling is now
named.

### 2. The aliases now agree, by asking the KIND

`needs_scaffolded_nros_toml(platform)` replaces `platform != "native"` at both
C and C++ sites. The file being written is a static QEMU-slirp network config
(`ip`, `mac`, `gateway`, `locator`) that a host build has no use for, so the
question was always "is this hosted", never "is this spelled `native`".

Deliberately NOT `&& platform != "posix"`: a third hosted alias would
reintroduce the bug, and the added condition would encode the symptom rather
than the question. Three tests — both spellings agree, a cross target still
gets the file, and an unknown platform is not assumed hosted.

### 3. The stale comment is corrected, and its conclusion survives

Measured rather than reasoned:

```console
$ nros new probe --platform native --lang rust
nros = { version = "*", default-features = false, features = [...] }
nros-board-linux = { version = "*", default-features = false }
```

Active, uncommented. So `--platform native` would satisfy that check today, and
the premise was stale. `plat="baremetal"` is unchanged — its *other* stated
reason (since #333, `nros new` refuses freertos for Rust) is current and is
what the choice now rests on.

## Left standing, deliberately: the SDK index's macOS dists

`nros-sdk-index.toml` carries real macOS artifacts — `qemu-macos-arm64.tar.zst`,
`arm-none-eabi-gcc-macos-arm64`, the Zephyr SDK macOS tarballs — with live URLs
and sha256s, plus `sdk_index`/`sdk_store` tests asserting `macos-arm64`
dispositions.

**Not touched, and this is a judgement rather than an oversight.** Phase-260 had
no wave covering the index, so these predate the decision rather than
contradicting it; the URLs point at published assets in a separate repository
(`nano-ros-sdk`); and the index describes what a package *offers per host key*,
which is a data-model question distinct from which hosts nano-ros supports.
Removing rows here would be a release-artifact decision, not a naming or
branch-hygiene one.

It is recorded so the inconsistency is visible: a macOS host can still be told
which toolchain tarball it would get, for a build that cannot succeed.

## Sweep

```sh
grep -rn 'macos\|APPLE\|apple-darwin' --include='*.rs' packages/cli/nros-cli-core/src | grep -v '/target/'
grep -n 'platform != "native"' packages/cli/cargo-nano-ros/src/scaffold.rs
```

## Provenance

Found by phase-401 W2 (the `native`/`posix`/`linux` audit), by the
`examples`+`cli` and `scripts`+`just`+`cmake`+`zephyr` scopes independently.
