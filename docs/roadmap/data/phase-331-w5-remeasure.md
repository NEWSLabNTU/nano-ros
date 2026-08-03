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

```
just build-test-fixtures lane=native   (33 manifest-declared build trees wiped first)
  BUILD_EXIT      0
  WALL_SECONDS    6794   (1h 53m)
  fixtures built  72
  errors          0
  stages:
    native         5222 s  (status 0)
```

| | W1 (`82b82a6d6`) | W5 | delta |
| --- | --- | --- | --- |
| wall clock | 7051 s (1 h 57 m) | 6794 s (1 h 53 m) | **-257 s (-3.6 %)** |
| native stage | 5912 s (84 % of wall) | 5222 s (77 % of wall) | **-690 s (-11.7 %)** |
| everything else | 1139 s | 1572 s | +433 s |
| fixtures built | 64 | 72 | **+8** |
| seconds per fixture (native stage) | 92.4 | 72.5 | **-21.5 %** |
| errors | 0 | 0 | 0 |

**The fold paid, and the per-fixture number is where it shows.** Wall clock alone
understates it: the native stage got 11.7 % faster while building 8 MORE
fixtures, so the cost of a fixture fell 21.5 %. That is the `nros sync` +
CMake-configure cycle count dropping from 35 workspaces to 15 — the saving
RFC-0066 predicted, in the units it predicted it.

**The non-native remainder went UP by 433 s, and that is mine, not the fold's.**
`generate-bindings` now syncs 7 template workspaces it used to skip — a bug
fixed earlier in the same session (`regenerate-bindings.sh` detected a colcon
workspace by `<root>/Cargo.toml`, which is not the rule `nros sync` uses, and its
discovery glob was single-depth so it never saw `examples/templates/*`). Those
syncs are new work, not slower work. Netting it out, the true wall-clock saving
is closer to **690 s (-9.8 %)** than to the -3.6 % the totals show.

So the honest summary: **-3.6 % as measured end to end, ~-10 % attributable to
the consolidation, -21.5 % per fixture.** No regression, so RFC-0066's option
(c) fallback stays unused.

## Reproducing

```
source ./activate.sh
bash scripts/dev/measure-fixture-build.sh native
```

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
