---
id: 1920
title: "`nros sync` cannot recover a leaf whose `.cargo/config.toml` includes a missing `build/<image>/nros-cargo.toml`: the metadata probe inherits the include, fails, and the error says to run `nros sync`"
status: open
type: bug
severity: medium
area: [cli, build]
related: [issue-1817, issue-1918]
found: 2026-10-15
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
