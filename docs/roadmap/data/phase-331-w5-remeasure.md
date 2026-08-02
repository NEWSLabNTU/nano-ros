# Phase-331 W5 re-measure — captured 2026-08-03

Compare against [`phase-331-w1-baseline.md`](phase-331-w1-baseline.md)
(2026-08-02, base `82b82a6d6`).

Method is now a script — `scripts/dev/measure-fixture-build.sh <lane>` — rather
than the prose bullet W1 left behind. It wipes every build tree the manifest
declares (derived from `fixtures-manifest.py list-workspaces` fields
`build_subdir` / `target_dir` / `codegen_out`, not globbed), rebuilds the CLI and
the launch resolver OUTSIDE the timed section, then times
`just build-test-fixtures lane=<lane>`.

## Static counts

| | W1 (`82b82a6d6`) | W5 | delta |
| --- | --- | --- | --- |
| workspace directories | 35 | 15 | **-20** |
| `[[workspace_fixture]]` rows | 86 | 93 | +7 |
| single-node `[[fixture]]` rows | 251 | 251 | 0 |
| tier1 coordinates | 10 | 10 | 0 |
| tier2 coordinates | 12 | 12 | 0 |
| `rust` node pkgs | 10 | 22 | +12 |
| `c` node pkgs | 9 | 18 | +9 |
| `cpp` node pkgs | 9 | 17 | +8 |
| `mixed` node pkgs | 10 | 18 | +8 |

**Rows went UP while directories went down, and that is the design working, not
a regression.** The fold removed 20 directories; W4 then spent some of that
budget on coverage the tree never had — `cyclonedds` and `xrce` on the language
workspaces, where 84 of 86 rows had been zenoh. A row is a fixture to build; a
directory is a `nros sync` + CMake-configure cycle. The trade RFC-0066 predicted
is exactly this shape: fewer configure cycles, not fewer fixtures.

The node-pkg counts roughly double because the themed workspaces' packages moved
into the language workspaces rather than disappearing. Same packages, one
workspace.

## Cold build

<!-- W5-BUILD-BLOCK -->

## Contamination — read the delta with these in hand

W5 does not compare two runs of the same tree, and pretending otherwise would
make the number worse than useless.

1. **W6 landed before W5.** The phase doc ordered W5 first precisely so the
   re-measure would not include the realtime/bridge fold; it does.
2. **Upstream moved underneath.** Between `82b82a6d6` and this run, phase-330
   (model generation), phase-332 (the play_launch repoint, `ros-launch-manifest`
   by git tag) and phase-333 (message deps become path deps; 26 leaves
   repointed) all landed. Any of them can move fixture build time.
3. **`generate-bindings` now syncs 7 template workspaces it previously skipped**
   — a fix landed in this same session (`regenerate-bindings.sh` used
   `<root>/Cargo.toml` to detect a colcon workspace, which is not the rule
   `nros sync` uses, and its discovery glob was single-depth so it never saw
   `examples/templates/*`). That is work W1's run did not do, added to W5's
   wall clock.

(3) is the only one that is straightforwardly additive; (1) and (2) can push
either way.
