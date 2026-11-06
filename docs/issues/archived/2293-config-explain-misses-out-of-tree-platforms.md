---
id: 2293
title: "`nros config explain --platform <p>` finds neither an out-of-tree platform package nor one `nros new platform` just scaffolded"
status: resolved
type: bug
area: cli, config, platform
related: [rfc-0049, rfc-0086, phase-400, phase-290, phase-537, 2292]
found: 2026-11-06
resolved: 2026-11-07
resolved_in: "fix-issue-2293 branch: one platform search path for board facts and config explain"
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

## Resolution

Both causes are fixed.

1. **One search-path derivation.** `board_facts::platforms_dir_entries(env,
   ws)` builds the `$NROS_PLATFORMS_DIR` entries: the caller's own value
   first, then `workspace_platform_roots(ws)`, each once. Board facts already
   built exactly this inline; it now calls the function, and so does
   `config explain`, which passes the result to
   `PlatformsTree::default_search_path` ahead of the SDK's
   `packages/platform` and `config`. `config explain` gains
   `--workspace <dir>`, defaulting to the cwd. `--platforms-dir` still pins
   one root.
2. **The scaffold writes `names`.** `scaffold_platform` writes
   `names = ["<name>"]`, as the in-tree descriptors do, so the package answers
   to `acmeos` and not to its directory name `nros-platform-acmeos`.

The `default_search_path` doc comment now names `$NROS_PLATFORMS_DIR`.

Test: `packages/cli/nros-cli/tests/config_explain_workspace_platform.rs`
scaffolds `nros new platform acmeos` into a temp workspace and runs
`config explain --platform acmeos` from it, then from elsewhere with
`--workspace`, with `NROS_PLATFORMS_DIR` removed. A negative control checks
that the same command from a directory without the package still refuses.
Both positive cases failed before the change (`unknown platform \`acmeos\``
and `unexpected argument '--workspace'`).

Not changed here: that refusal prints `no <sdk>/packages/platform/acmeos/nros-platform.toml`,
a path derived from the platform's NAME. phase-468 W1 removed that shape from
the build-script message (`unanswered_platform_message`); the
`ConfigError::UnknownPlatform` text `config explain` prints still has it.
