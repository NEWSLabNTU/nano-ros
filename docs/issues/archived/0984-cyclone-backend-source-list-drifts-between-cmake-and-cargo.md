---
id: 984
title: "One archive, two authored source lists — `nros_sertype.cpp` reached the
  cmake target and not the cargo one, so every Rust fixture failed to link"
status: resolved
type: bug
area: rmw, build
severity: high
found: 2026-09-02
related: [issue-0970, issue-0979, issue-0088, issue-0475]
---

## Symptom

`just build native` fails at `fixture-0018`
(`bridge-zenoh-to-cyclonedds-fwd`):

```
rust-lld: error: undefined symbol: nros_rmw_cyclonedds::create_nros_sertype(dds_topic_descriptor const*)
  >>> referenced by publisher.cpp:148
  >>>   115f693ad0506440-publisher.o:(nros_rmw_cyclonedds::publisher_create(...))
  >>>   in archive .../out/libnros_rmw_cyclonedds.a
```

The reference comes from INSIDE the archive: a TU compiled against a
declaration whose definition was never compiled in beside it.

## Not a stale archive

Worth stating, because the shape (`-Wl,--whole-archive -lnros_rmw_cyclonedds`,
a raw link flag) is issue 0475's, and 0475's remedy is a rebuild edge. It is
not that. The archive was written minutes earlier by the run that failed, and
its member list settles it:

```
$ ar t libnros_rmw_cyclonedds.a | grep -E 'sertype|publisher'
115f693ad0506440-publisher.o
115f693ad0506440-subscriber.o
115f693ad0506440-sertype_min.o          <- and no nros_sertype.o
$ nm -C libnros_rmw_cyclonedds.a | grep -c 'T .*create_nros_sertype'
0
```

Fresh archive, missing member. Nothing was stale; a source was never compiled.

## Root cause: the archive has TWO authored source lists

`libnros_rmw_cyclonedds.a` is built two ways, and each names its sources by
hand:

| built by | list | linked by |
| --- | --- | --- |
| cmake | `nros-rmw-cyclonedds/CMakeLists.txt` `target_sources` | C/C++ examples |
| cargo | `nros-rmw-cyclonedds-sys/build.rs` `let cpp_files` | Rust fixtures |

`b4858f941` (`feat(#0970): the Cyclone backend registers its own sertype`) added
`src/nros_sertype.cpp` to the CMake list — one `+1` line in its diffstat — and
not to `cpp_files`. So every C/C++ example kept linking and every Rust fixture
could not, which is why it looked like nothing was wrong.

`build.rs` was already carrying the warning, eight lines below the list:

> The CMake target adds these too (see `nros-rmw-cyclonedds/CMakeLists.txt:95`);
> without them the vendored cargo build leaves the symbols undefined.

A comment is not a check. This is the third instance of "one derivation, two
spellings" found in two days, after issue 0978 (the generated-header mirror) and
issue 0981 (the RX size-bound rule) — and like 0981, the commit that broke it
had a note explaining precisely the hazard it then walked into.

## Why it surfaced only now

It did not surface when `b4858f941` landed because the Rust fixture stage was
already dying earlier, in `nros-node`'s build script (issue 0979). Fixing 0979
let the lane reach LINKING for the first time since, and this was waiting there
— the same way 0978's fix uncovered 0979. Three failures deep in one lane, each
hidden by the one in front of it.

## Fix

`nros_sertype.cpp` added to `cpp_files`.

Gate: `just check cyclone-backend-sources` →
`scripts/check-cyclone-backend-sources.py`, on the fast line. It extracts the
`src/*.cpp` set from the cmake target and the `let cpp_files` array from
`build.rs` and requires them equal, naming the direction of any difference and
what it will break. Pure text, no build.

The `bridge/` list is deliberately not compared: those TUs are only in the cargo
build, and `build.rs` says so.

Proven non-vacuous: against the pre-fix tree it reports

```
check-cyclone-backend-sources: the two lists that build libnros_rmw_cyclonedds.a disagree.
  only in CMakeLists.txt: nros_sertype.cpp
      -> a RUST fixture will fail to link against symbols this TU defines,
         while every C/C++ example keeps working (issue 0984).
```

The script runs its own selftest on the normal path, per
`check-gate-selftests`.

## Sweep

Build scripts carrying a C++ source list mirrored by a CMake target: this is the
only one in the tree (`git grep -l '\.cpp"' -- '*/build.rs'` yields this crate
and one CLI source file that merely shares the filename).

## Acceptance

* [x] `cpp_files` and the cmake target name the same backend TUs.
* [x] A gate that fails on the pre-fix tree and passes after.
* [x] `just build native` links `fixture-0018`.
