---
id: 2318
title: "Every CMake leaf recompiles the same host build tooling into its own cargo directory"
status: open
type: tech-debt
severity: medium
area: [build, ci, cmake]
related: [0616, 1353, 2191, phase-543]
found: 2026-11-07
---

**Home:** [phase-543](../roadmap/phase-543-nextest-gates.md), whose W4 records
`build/corrosion-cargo` growing 51 -> 160 G over five gate runs ("nothing
prunes them"). This issue is the measurement of WHY one run is that large.

## Measured

Runner newslab-133, 2026-11-07. The fixture tree had been evicted to its
80 GiB budget about an hour earlier, so everything below came from one tier-2 job.

| Area | Size |
|---|---|
| `build/corrosion-cargo` | 27 G (37 G before the eviction) |
| `build/cargo-fixtures` | 27 G (before the eviction) |
| `build/cmake-fixtures` | 11 G (before the eviction) |
| `build/sizes-probe` | 5.5 G, 26 per-leaf dirs |
| `packages/cli/target/debug` | 15.5 G, one job's `check cli-tests` binaries |

Before that eviction the fixtures area was 184 GiB, and on a volume shared with
other work it reached 0 bytes free (tier-2 run 42320747407 died on
`No space left on device` in `check::default`).

### One cargo directory per leaf, holding the same units

`build/corrosion-cargo/metadata-probe/5eab8b54fb38/` is ONE shared-cargo key
(`features=…|rmw=zenoh|profile=nros-relwithdebinfo|target=x86_64-unknown-linux-gnu|…`),
and under it sit **48** per-leaf directories, `nros-c-image_<hash>` and
`nros-cpp-image_<hash>`, ~0.4–1.0 GB each, 20.6 GB together.

The same cargo unit is built again in each: `libnros_macros-b5224629fcfabc5c.so`
(one metadata hash, so the same features, deps and profile) is present in 12 of them.

### Most of each directory is HOST tooling

In that key group:

- host side (`nros-relwithdebinfo/`: build scripts, proc-macros, their deps): **16.4 GB**
- target side (`x86_64-unknown-linux-gnu/nros-relwithdebinfo/`, the image code): **4.2 GB**

In one `nros-cpp-image` directory the largest host entries are
`libnros_macros-*.so` 71 MB, `libnros_orchestration_ir` 30 MB,
`ros_launch_manifest_{model,types}` 23 + 21 MB, and a build-script binary per
`-sys`/codegen crate at 25–30 MB each (`zpico-sys`, `nros-c`, `nros-rmw-cffi`,
`nros-node`, `nros-params`, `nros-rmw-zenoh`, …).

Most of those bytes are DWARF. `strip -g` on two of them:

| artifact | as built | `strip -g` |
|---|---|---|
| `zpico-sys` `build-script-build` | 31.0 MB | 10.7 MB |
| `libnros_macros-*.so` | 73.9 MB | 19.9 MB |

The profile is `nros-relwithdebinfo` (`inherits = "release"`, `debug = 1`), and
its build scripts and proc-macros carry that debuginfo.

### Second sighting: runner newslab-118, 2026-11-07

Tier-2 run 42364894798 (workflow_dispatch on 9d0bd000a8), job 128383215067,
step `just ci matrix` (`check::default`), 11:39–12:23 UTC.
- At 10:52 the job's disk report read `fixtures: 279.8 GiB used / 280.0 GiB
  budget (high-water 496.1 GiB)`, `cargo-targets: 80.6 GiB`, and 968 G free
  in `/home/runner/_work`.
- About 50 minutes into `check::default`, 62 lines read
  `gate-units.sh: line 79: echo: write error: No space left on device`, and
  `ld terminated with signal 6 [Aborted]` (`nros-platform-cffi` build
  script).
- Several gates had failed before that first ENOSPC line:
  `scaffold-builds` (19 variants), `template-copy-out` (11) and
  `census-hooks-complete`. Most of them are probably the same exhaustion, but
  the earliest ones (`scaffold-builds#rust_project_server` at 11:50) were not
  separated from it.
- Tier 1 in the same run went green, so this is about disk, not about the code
  at 9d0bd000a8.
- Not measured: which filesystem filled. 968 G was free on the work volume at
  the start, so either the 61 parallel gate units wrote most of it, or the
  units' scratch (`/tmp/nros-scaffold-builds.*`,
  `/tmp/nros-template-copy-out.*`) sits on a smaller filesystem inside the
  container. A `df` of `/tmp` and of `_work` at the failure point would decide
  which.

## Why the directories are separate

Separation is deliberate. Issue 0616: `-C metadata` includes the path spelling a
crate was reached by, so a target dir serves exactly ONE workspace root, and two
roots sharing one bind a duplicate `#[global_allocator]` intermittently. Each
leaf's generated image crate is its own root, so each gets its own directory.
Sharing the directory is not the fix.

What is NOT forced by 0616 is that every leaf pays for the HOST half again. The
host units do not differ between leaves of one key; only their path spelling does.

## What would close it

Either one would bound a job's footprint by the number of KEYS, not leaves:

1. **Smaller host units.** Drop debuginfo from build scripts and proc-macros
   (`[profile.nros-relwithdebinfo.build-override] debug = 0` or
   `"line-tables-only"`), roughly 3x on the host half. This needs a study first: confirm that no
   size probe, `mem-report` or sizing step reads DWARF from a HOST artifact.
   The image's own debuginfo is untouched by `build-override`, but that has to be
   checked, not assumed.
2. **Build the host half once per key.** A shared, keyed directory for the
   host-only units, with the per-leaf directory for the target half only. This is
   a 0616-shaped change and needs the same measurement (a duplicate-symbol link
   test under parallel leaves) before it is believed.

Acceptance is a BUILD measurement: one tier-2 job's `build/corrosion-cargo`
before and after, on a tree evicted to empty first, plus the existing link
gates green.

## Interim

The sweep bounds what persists BETWEEN jobs (fixtures budget, plus a per-root
cap on the host target dirs that also caps `packages/cli/target`), not what one
job writes. A runner needs room for one job's peak.
