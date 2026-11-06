---
id: 2293
title: "`nros config explain --platform <p>` finds neither an out-of-tree platform package nor one `nros new platform` just scaffolded"
status: open
type: bug
area: cli, config, platform
related: [rfc-0049, rfc-0086, phase-400, phase-290, phase-537, 2292]
found: 2026-11-06
---

**Home: none yet.** Reported by the phase-537 W12 book session with an
in-tree CLI.

## Symptom

The scaffold's own template tells the porter to run
`nros config explain --platform <name>` to debug the knob ladder. For a
platform package in the user's workspace, and for a fresh
`nros new platform acmeos` (which writes `nros-platform-acmeos/`), the
command reports the platform as unknown.

## Code site

There are two separate causes:

1. **No workspace on the search path.**
   `packages/cli/nros-cli-core/src/cmd/config.rs`, `explain()`, builds the
   search path from `PlatformsTree::default_search_path(&repo,
   $NROS_PLATFORMS_DIR)` (`packages/tooling/nros-platform-config/src/platform_config.rs`).
   That covers the env var, `<sdk>/packages/platform` and `<sdk>/config`,
   and never the user's workspace. Phase-400 W1 fixed this for board facts:
   `cmd/board_facts.rs` adds `workspace_platform_roots(ws)` to
   `NROS_PLATFORMS_DIR`, because "a platform package in the user's own
   workspace … resolved only if a human exported the variable by hand".
   `config explain` never got that fix, so it is the second reader of one
   search path, with a narrower reach.
2. **The scaffold writes no `names`.** `PlatformsTree::load` keys a
   descriptor by `names[0]` and falls back to the directory name.
   `cmd/new_platform.rs` `scaffold_platform()` writes `nros-platform.toml`
   with no `names` field into `nros-platform-<name>/`. Even on the search
   path it registers as `nros-platform-acmeos`, so `--platform acmeos`
   misses it. The in-tree descriptors avoid this by stating
   `names = ["<name>"]`.

A smaller drift: the doc comment on `default_search_path` names
`$NROS_PLATFORM_PATH`, but every caller passes `$NROS_PLATFORMS_DIR`.

## Fix direction

- Give `config explain` the workspace roots that board facts uses:
  `workspace_platform_roots` (or one shared function that both call) over a
  `--workspace` that defaults to the cwd. The result is one search-path
  derivation, not two.
- Have `scaffold_platform` write `names = ["<name>"]`.
- Fix the env-var name in the `default_search_path` doc comment.
- Add a test that scaffolds a platform into a temp workspace and runs
  `config explain --platform <name>` from it.
