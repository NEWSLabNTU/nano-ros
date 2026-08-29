---
id: 875
title: "The repeated picolibc build in every Zephyr west leaf is 0.2% of the build,
  and rlsf cannot displace it — recorded so the idea is not re-derived"
status: resolved
type: tech-debt
area: build
related: [issue-0805, issue-0616, phase-340, phase-371, phase-391]
---

## The lead

Reported as *"the newlib build is repeated"*, with the hypothesis that newlib is
there only to supply `malloc`, so the [phase-391](../../roadmap/phase-391-allocation-unification-and-tier-model.md)
rlsf heap could displace it and take the repeated build with it.

The repetition is real. Zephyr builds **picolibc** — a newlib fork, hence the
`newlib/libc/...` object paths that made this look like newlib — from source in
every west build dir: 932 objects each, 67 build dirs, ~62,000 compilations, and
every one of those dirs is the same board (`native_sim/native/64`). It reads as
pure waste.

Both halves of the hypothesis fail on measurement. Recorded here because the
idea is a natural one to have twice.

## It is 0.2% of the build

Bucketing every ninja edge across the 66 build dirs that have a `.ninja_log`:

| bucket | edges | edge-seconds | share |
| --- | --- | --- | --- |
| nros/rust (cargo wrapper edges) | 18,611 | 360,351 | **91.7%** |
| zephyr kernel | 54,152 | 30,269 | 7.7% |
| other | 8,724 | 1,327 | 0.3% |
| picolibc | 50,382 | 934 | 0.2% |

picolibc is a median of **14.6 edge-seconds per build dir**. One cargo edge in
`build-cpp-action-client-zenoh` is **631.6 seconds**. Eliminating picolibc
entirely buys 0.2%, and eliminating it is not on offer anyway — see below.

The objects do genuinely differ between dirs, for incidental reasons: `-Og` vs
`-Os`, `-fno-printf-return-value`, per-app `-imacros autoconf.h`, per-app
`-fmacro-prefix-map`, per-dir `-isystem`. So this is not even a case where
identical work is repeated verbatim.

## picolibc is not there only for `malloc`

Linking `build-c-listener-zenoh/zephyr/zephyr.exe` against its `libc.a`:

```
picolibc archive members:          894
members contributing to the image:  37   (4.1%)
of those, the allocator:             3   (nano-malloc-{malloc,free,realloc}.c.obj)
```

The other 34 are `vfprintf`, `abort`, `__assert_no_args`, `getenv`, `environ`,
`__cxa_atexit`, `__retarget_lock_*`, `__memcpy_chk`, `__dso_handle`. Replacing
`malloc` with rlsf removes **3 of 37** linked members and the library still
links.

And decisively for the build-time argument: **the archive is compiled whole
regardless of what the linker later picks from it.** Dropping three members from
the image does not drop three compilations from the build. The rlsf swap changes
the build by exactly zero.

rlsf stays worth doing on phase-391's own grounds — O(1) worst case for the
safety island, one arena behind one funnel, a net −974 B of flash. It is not a
build-work reduction, and phase-391 does not claim it is.

## The cargo half is already settled — see issue 0805

The profile points away from picolibc and at the 91.7%: `zephyr/cmake/
nros_cargo_build.cmake` pins `CARGO_TARGET_DIR=${CMAKE_BINARY_DIR}/nros-rust`,
which is per build dir, so each of the 67 leaves builds the Rust side into its
own tree (3.9 GB in `build-c-listener-zenoh`, 2.8 GB in
`build-cpp-action-client-zenoh`, 154 GB for `zephyr-workspace` today).

**That is not a new finding and it must not be re-opened here.**
[Issue 0805](0805-every-cpp-leaf-rebuilds-the-same-staticlib.md) wired
`NROS_SHARED_CARGO_ROOT` across `native`, `freertos`, `nuttx` (both arches),
`threadx-linux` and `threadx_riscv64`, then investigated zephyr *with the intent
of wiring it the same way* and concluded, on 2026-08-27, that sharing is the
wrong fix there. Four independent reasons, all still standing:

- the per-image generated headers live **inside** the target dir
  (`nros-c-generated/nros/nros_config_generated.h` carries this image's
  `NROS_EXECUTOR_STORAGE_SIZE`), so a shared dir hands one image another's
  sizes — the mirror class that has burned this repo six times, and it fails as
  a wrong runtime rather than a build error;
- there is little to reuse: 199 `libnros_c*.a` across the workspace are **147
  distinct**;
- a sharing key cannot be shown complete, because Kconfig reaches deep build
  scripts through `$DOTCONFIG` and does not reliably reach cargo's fingerprint
  (issue 0460);
- `nros_cargo_build.cmake` already carries a measured 0616 guard saying sharing
  there "bought nothing" and "produced only the collision".

The disk half also has an owner: the mass is each leaf accumulating one cargo
directory **per profile** and never dropping the ones a later configure stopped
naming. `just gc-zephyr-builds [--prune]` reclaims those; 149 GB was pruned on
2026-08-27, and the 154 GB measured here is that re-accumulating.

## What is actually left

Nothing in this issue. The two open threads both belong elsewhere:

- **the warm floor**, issue 0805's own remaining item — ~70 cargo invocations
  re-scanning fingerprints at ~7 s each. It needs *fewer invocations* (the
  prebuilt-artifact shape), which no target-dir or cache change can give it.
- **rlsf**, phase-391 W5, on its own merits.

## Why this is filed at all

*Repeated* and *expensive* are independent properties, and the eye is drawn to
the first. 62,000 compilations of the same library for the same board is the
most visibly wasteful thing in the tree and one of the cheapest; the 91.7% is
one edge per build dir and looks like nothing in a log.

The same `.ninja_log` that made picolibc look like the problem is what ruled it
out, in about a minute. Profile before proposing.

## Reproduce

```
python3 - <<'PY'   # bucket every ninja edge in zephyr-workspace by subsystem
import glob, collections
tot = collections.Counter(); cnt = collections.Counter()
for lg in glob.glob("zephyr-workspace/build-*/.ninja_log"):
    for line in open(lg, errors="replace"):
        if line.startswith("#"): continue
        p = line.split("\t")
        if len(p) < 5: continue
        ms = int(p[1]) - int(p[0]); out = p[3]
        k = ("picolibc" if "modules/picolibc" in out else
             "zephyr-kernel" if "/zephyr/" in out or out.startswith("zephyr/") else
             "nros/rust" if ("nros" in out or "cargo" in out) else "other")
        tot[k] += ms; cnt[k] += 1
for k, v in tot.most_common():
    print(f"{k:<15}{cnt[k]:>8}{v/1000:>12.1f}s")
PY
```

For a single build, `just profile <build-dir>` (`nros-build-profile`,
phase-251) is the supported reader; the snippet above is the cross-build
aggregate, which is what makes the per-leaf repetition visible in the first
place.

## sccache, measured (2026-08-29)

Checked because "share the compile work with a content-addressed cache" is the
other natural proposal, and because a first pass here wrongly reported sccache
as absent on this host. It was not: `activate.sh` puts the SDK store's `bin` on
PATH and sets `SCCACHE_DIR=/tmp/nros-build-aeon/sccache`, which already held
11 GB against a 10 GiB cap. The check that said otherwise had not sourced
`activate.sh` — the one step `just doctor` exists to enforce.

A/B on `build-c-listener-zenoh`, deleting the whole cargo target dir each time:

| run | wall | hit rate |
| --- | --- | --- |
| `RUSTC_WRAPPER` unset | 15 s | — |
| sccache | **9 s** | 96.45% (Rust 96.23%, C/C++ 100%) |
| sccache, repeat | 9 s | 96.45% |

So sccache is already doing its job on this tree, and the ceiling is visible in
the same output:

```
Non-cacheable reasons:
crate-type   49
```

That is `--crate-type=staticlib` — `libnros_c.a` itself, the artifact the leaf
exists to produce. Issue 0805 states this ("sccache cannot absorb it"); the
counter is the confirmation.

One correction to the framing above, in the same spirit as the picolibc result.
The 360,351 edge-seconds are dominated by **cold first builds** — a `.ninja_log`
keeps the last run per output, and most of these dirs were built once. Warm, with
sccache, a full cargo target-dir wipe on this leaf costs 9 s. The per-leaf
duplication is therefore a *cold-build* cost, which narrows who should care about
it: CI on a fresh runner, and a developer's first build after a wipe. Not the
edit-build loop.
