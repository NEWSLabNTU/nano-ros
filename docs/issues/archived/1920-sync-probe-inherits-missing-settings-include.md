---
id: 1920
title: "`nros sync` cannot recover a leaf whose `.cargo/config.toml` includes a missing `build/<image>/nros-cargo.toml`: the metadata probe inherits the include, fails, and the error says to run `nros sync`"
status: resolved
type: bug
severity: medium
area: [cli, build]
related: [issue-1817, issue-1918]
found: 2026-10-15
resolved: 2026-10-15
---

## Measured

Found while gating issue 1918: `just ci gate`'s `linux-check` stopped at
`linux check: \`nros sync\` failed in examples/linux/rust/action-client`.

A leaf's gitignored `.cargo/config.toml` (written by sync) carries
`include = [".../nros-patch.toml", "../build/<image>/nros-cargo.toml"]`. The
metadata probe crate is built INSIDE the leaf
(`<leaf>/build/nros-metadata/metadata-probe/<component>/`), so cargo's config
discovery walks up and loads that include. When the settings file is missing
and the probe must re-run (the component's sources changed), the probe fails to
load its configuration, and `metadata_refresh` then reports issue 1817's
resolution error:

```
Error: native_action_client::fibonacci_action_client: cargo cannot resolve .../action-client/Cargo.toml:
.../.cargo/config.toml includes .../build/linux/nros-cargo.toml, which does not exist — run `nros sync`
```

The file sync would write next (`sync: settings → .../build/linux/nros-cargo.toml`)
is written AFTER the probe, so every later `nros sync` hits the same wall: the
state is absorbing, and the remedy the error names is the command that fails.

Reproduced on an untouched leaf (`examples/linux/rust/listener`): with
`.cargo/config.toml` naming `../build/linux/nros-cargo.toml`, delete that file,
change a source line, run `nros sync` — same error. Deleting the generated
`.cargo/config.toml` and re-running sync recovers (`sync: done.`, settings
written). The ordinary way into this state is removing a leaf's `build/` (or the
one image directory) while keeping its `.cargo/config.toml`.

## What would close it

Write the image's `nros-cargo.toml` before the probe runs (it depends on the
image, not on the probe's output), or run the probe with a cargo config that
does not inherit the leaf's include. At minimum the error must not tell the user
to run the command that is failing.

## Resolution

**Fixed by isolation, not by ordering.** The proposed ordering fix ("write the
settings before the probe, since it depends on the image, not on the probe")
was measured against the writer and does not hold: layer 2 of the settings
file is `leaf_entity_env::leaf_env`, which derives the pool knobs and entity
facts from the probe's sidecar, and the sizing descriptor written beside it
reads the same inventories. Writing it first would write the previous
probe's facts.

What the probe needs from the leaf's config is only its `[patch]` rows, and
`write_patch_block` writes every row the settings file carries into the leaf's
own managed block and the central patch include BEFORE the probe runs. So the
probe now skips the settings-include family (`../build/<image>/nros-cargo.toml`),
present or absent. A stage never reads a later stage's output, and its rows no
longer depend on whether that stage has run. Measured on
`examples/linux/rust/listener`, `mps2-an385-baremetal/rust/listener` and
`mps2-an385-freertos/rust/listener`: the settings rows (`{nros}`, or
`{nros, nros-rmw-zenoh}`) are a subset of the rows written before the probe,
and the listener's `nros-probe-patch.toml` is byte-identical to the baseline.
The family has one spelling, `leaf_settings::is_settings_include`. The writer
that re-decides the `include` array uses it, and so does the probe that skips
the family.

An error raised inside `nros sync` no longer names `nros sync` as its remedy.
The missing-`include` error says to restore the file or remove the entry. The
central-patch error in `write_patch_block` says that re-running cannot help.

Symptom proof: the listener repro (settings file deleted, a source line
changed, `.cargo/config.toml` left as it was) gives `sync: source metadata — 1
rebuilt`, `sync: settings → …/build/linux/nros-cargo.toml`, `sync: done.`, rc
0. The rewritten settings file is identical to the original.

Gate: `packages/cli/nros-cli-core/tests/sync_recovers_missing_settings_include.rs`
(the `cli-tests` lane) builds the state on a copy of the listener. It syncs,
deletes the settings file, edits a source so the probe must re-run, then syncs
again. It asserts that the second sync succeeds, writes the file back, and
really re-ran the probe. Unit tests in `metadata_build` pin the skip (absent
and present) and the message. Mutation: with the skip disabled, the e2e test
and the unit test give rc=101; restored, they give rc=0. With the message
re-appending "run `nros sync`", the unit test gives rc=101.

Siblings left alone because neither is on the sync path and neither is
absorbing (sync recovers both): `nros build`'s pre-build `cargo update
--precise` (`orchestration/drift.rs`, cwd = the leaf) runs before `nros build`
writes the settings, so with the file missing it cannot realign a drifted lock
and reports the drift. `NrosConfig::from_cargo_metadata` (`cargo metadata` with
cwd = the workspace) fails on the same missing include for the verbs that
load it.
