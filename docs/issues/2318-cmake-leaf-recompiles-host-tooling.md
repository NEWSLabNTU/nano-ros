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

## `just generate-bindings` configured every image (2026-11-11)

Since phase-530 W1 (`fb7a00ec7e`) `scripts/regenerate-bindings.sh` has
called `nros configure` per workspace. That is C1 (codegen, the patch
config, the metadata probe), which is what the script wants, plus every
image's stages. For a C/C++ cross image those stages take the entity
census, which builds and runs a native image, and the W3 shared host
configure. Measured on cold `examples/workspaces/cpp` with `--no-metadata`:
173 s, 645 crates compiled, 5.4 GB written, and the log reads
`entity-census take … taking the census with linux_action_client`. That is
what made the CI step 31-47 min (issue 2141) and filled a hosted runner's
disk.

**Fix: `nros dev bindings`** (contributor tier, RFC-0120 D2 note). It runs
C1 alone under the project lock and records no stamp, so `nros build` still
configures each image. `regenerate-bindings.sh` calls it. The user verb
stays `nros configure`.

Measured on one host (newslab-118, 32 cores), each run cold: `build/` deleted
and the ignored output under `examples/` and `packages/testing/` cleaned
first, then `just generate-bindings`:

| | wall | `build/` | `examples/` | `build/sizes-probe` |
| --- | --- | --- | --- | --- |
| main `96be8a12cf` (`nros configure`) | 1,408 s | 40.1 GB | 15.5 GB | 6.4 GB |
| `nros dev bindings` | 614 s (−56 %) | 35.6 GB | 3.0 GB | 2.2 GB |

Total output fell from 55.6 GB to 38.6 GB.

**What remains is this issue's own subject.** 30 GB of the 35.6 GB is
`build/corrosion-cargo/metadata-probe`, the C/C++ metadata probe that C1
itself runs. Under one key (`5eab8b54fb38`, 23 GB) there is one
`nros-c-image_*` / `nros-cpp-image_*` directory per C/C++ workspace, about
1 GB each, all with the same features. The fixes above (host debuginfo, a
shared host half) are now the next lever for `generate-bindings` too.
