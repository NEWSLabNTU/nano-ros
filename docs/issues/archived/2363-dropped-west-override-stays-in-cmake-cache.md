---
id: 2363
title: "A west_overrides knob removed from a fixture row stays in the build dir's CMakeCache, so the transient override persists"
status: resolved
type: bug
area: [zephyr, cmake, testing]
severity: medium
found: 2026-11-08
related: [2362]
---

**Home: [phase-531 W6](../../roadmap/phase-531-workspace-first-project-layout.md)**, found in batch 8.

RFC-0103 D2 makes a knob override from the command line
(`-DNROS_OVERRIDE_<ID>=<v>`, a fixture row's `west_overrides`) **transient**:
it belongs to that configure and nothing else. On an incremental build it is
not transient. CMake caches every `-D`, so once a row stops passing the
override, the old value is still in the build directory's `CMakeCache.txt`
and the image keeps building with it.

## Repro (measured 2026-11-08)

1. Build `zephyr-cortex-m-c-boot-report-zenoh` with
   `west_overrides = { EXECUTOR_ARENA_SIZE = "32" }`.
2. Remove that line from `examples/fixtures.toml` (issue 2362 did this to
   park the row).
3. Rebuild that row:
   `NROS_ZEPHYR_FIXTURE_FILTER='^build-cortex-m-c-boot-report' just zephyr build-fixtures`.

What happens:

- `<build>/CMakeCache.txt` still holds
  `NROS_OVERRIDE_EXECUTOR_ARENA_SIZE:UNINITIALIZED=32`.
- The configure still prints `knob override(s) from the command line:
  EXECUTOR_ARENA_SIZE (transient; RFC-0103 D2)`.
- The build fails exactly as before the edit, in the arena oracle at 32 B.

The row signature (`overrides=` in `zephyr-fixture-leaves.sh`) does change,
so the emitter knows the inputs moved. Nothing then drops the stale cache
entry.

## Fix direction

When the module reads `NROS_OVERRIDE_*`, it should also clear every cached
override the current command line did not pass. The nros module already does
this kind of bookkeeping for `EXTRA_CONF_FILE`, through
`NROS_IMAGE_CONF_FILES` in `zephyr/cmake/nros_image_kconfig.cmake`. The
other way is to have the fixture runner pass `-U NROS_OVERRIDE_*` before its
`-D`s. Do NOT wipe the build directory: a re-configure is the escape hatch
(CLAUDE.md, the `rm -rf` antipattern). The workaround is
`cmake -U NROS_OVERRIDE_<ID> <build-dir>`.

Acceptance: the repro above builds with the derived arena, and a test pins
it (configure with an override, then without, and the second configure
reports none).

## Resolution (2026-11-10)

The first fix direction, made unconditional: the module consumes each
override.

- `_nros_image_consume_overrides` (`zephyr/cmake/nros_image_kconfig.cmake`)
  harvests every `NROS_OVERRIDE_<ID>` cache entry, keeps its value for this
  configure, and removes the entry at once.
- A later configure applies exactly the overrides its own command line passes.
- The hook runs once per configure (`zephyr/modules/modules.cmake`), so
  nothing later in the same configure needs the cache entry.
- The status line now says "transient: this configure only".

Nothing depends on a wipe: the fix is a re-configure of the same dir.

Measured with `west build --cmake-only` on one build dir (a copy of the
`cpp-port-minimal-publisher` Zephyr template, `mps2/an385`, Zephyr 3.7):

| configure | main | this change |
| --- | --- | --- |
| 1: `-DNROS_OVERRIDE_ACTION_INBOX_DEPTH=7` | `CONFIG_NROS_ACTION_INBOX_DEPTH=7` | `=7` |
| 2: same dir, no `-D` | still reports the override; `=7`; `NROS_OVERRIDE_ACTION_INBOX_DEPTH:UNINITIALIZED=7` in the cache | reports none; `=4` (the default); not in the cache |

The trade-off is deliberate. A configure that ninja re-runs on its own during
a later build, without the `-D`, builds without the override too. That is the
same rule ("this configure's"), and the fixture runner passes a row's
overrides on every configure it starts.

**A second defect, found on the way.** `fixtures-manifest.py west-overrides`,
which the fixture runner reads for every row's `west_overrides`, crashed with
`UnboundLocalError`. 2865cc77c5 made it call `west_rows()`, and a local
variable of the same name later in `main()` shadowed the function. The runner
read it through a process substitution, which discards the status, so every
row built with no override and nothing said so.

- The local is renamed (`west_records`).
- The runner now reads the listing into a variable first and stops when it
  fails.

Test: `just check zephyr-transient-overrides`
(`tests/zephyr-transient-overrides-tests.sh`), a probe project that includes
the module file. It configures with two overrides, then the same dir with
none, then with one, and requires each configure to see exactly its own.

- It also requires that no cached override is left behind, that the hook
  harvests through the function, and that the override listing runs.
- Mutation-tested: without the `unset`, it fails.
- Against main's `fixtures-manifest.py`, case F fails with the traceback.
