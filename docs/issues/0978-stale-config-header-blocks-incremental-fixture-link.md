---
id: 978
title: "The corrosion config header is a BUILD-SCRIPT side effect, so a shared cargo
  group refreshes it in exactly one leaf — the other 19 fail to link against an
  archive whose sizes moved"
status: open
type: bug
area: cmake, build
related: [issue-0616, issue-0369, issue-0740, issue-0834, phase-340]
---

## Symptom

An incremental `just build native` (this host, 2026-09-01) fails four fixture
leaves with nothing but a bare undefined reference:

```
/usr/bin/ld: CMakeFiles/cpp_listener.dir/src/main.cpp.o:(.data.rel.ro+0x8):
  undefined reference to `nros_config_variant_sz_f3c40eb64e98fb7d'
collect2: error: ld returned 1 exit status
```

`fixture-linux-c-zenoh`, `fixture-linux-c-xrce`, `fixture-linux-cpp-zenoh` and
`fixture-linux-cpp-xrce` all die this way, which stops the whole native lane —
so no native fixture downstream of them rebuilds, and the ones that already
exist stay at their previous timestamps looking perfectly serviceable.

## This is the size anchor working, not misfiring

`nros_config_variant_sz_<hash>` is issue 0369's anchor: an FNV over the
`(name, value)` size pairs the generated header ships
(`variant_suffix_from_sizes`, `nros-build-helpers/src/shared.rs`). Compiling
against one set of sizes and linking an archive built with another is meant to
be a LINK error rather than a silent `_opaque` overflow — the whole
0088/0114/0122/0123/0245/0268 family.

It caught a real disagreement. In `examples/native/cpp/listener/build-zenoh`:

| artifact | anchor | mtime |
| --- | --- | --- |
| `libnros_c.a` | `sz_cd6bc387c5d734f9` | 2026-09-01 03:24 |
| `libnros_cpp.a` | `sz_cd6bc387c5d734f9` | 2026-09-01 03:24 |
| all four `nros_config_generated.h` copies | `sz_f3c40eb64e98fb7d` | **2026-08-26 20:23** |

Both archives rebuilt today with new sizes; every header in the leaf is six
days old.

The other anchor in the same object is the diagnostic that narrows it. `nm -u`
on `main.cpp.o` wants two symbols:

```
U nros_config_variant_sz_f3c40eb64e98fb7d                          <- MISSING
U nros_cpp_config_variant_alloc_env_..._rmw_zenoh_cffi_ros_humble_std  <- present
```

The FEATURE-slug anchor matches. So this is not the feature mismatch the
panic text in `write_header_if_absent_or_verify` tells you to go and check
("usually this means nros-c and nros-cpp were built with different features") —
same features, different sizes, one half refreshed.

## Root cause: a build-script side effect under a shared cargo group

There are two writers of this header, and only one of them has staleness logic:

* `write_header_if_absent_or_verify` → the cargo target dir. Compares defines,
  and rewrites when the stamps prove "same probe path, rebuilt since".
* `write_header_to_corrosion` → `$CORROSION_BUILD_DIR/<filename>`, an
  **unconditional** `write_atomic`. This is the copy the fixture actually
  compiles against.

Both run inside `nros-{c,cpp}`'s build script. `CORROSION_BUILD_DIR` is
per-LEAF (`<leaf>/build-zenoh/nano_ros/packages/api/nros-cpp`), but since
phase-340 the cargo `--target-dir` is a SHARED GROUP across leaves
(`<leaf>/build-*/cargo/nano-ros_1147c`, the same directory for every leaf in
the group).

So cargo runs the build script **once per (crate, feature set)**, not once per
leaf. The first leaf to build gets its corrosion header rewritten; every other
leaf in the group is told the crate is fresh, the script never runs, and its
corrosion header keeps whatever it held — while all of them link the shared
archive, which now carries the new anchor.

The survey across the 20 native leaves says exactly that:

```
 1 x sz_cd6bc387c5d734f9   2026-09-01 03:23   examples/native/c/talker/build-zenoh/...
19 x sz_f3c40eb64e98fb7d   2026-08-26 20:21   (everything else)
```

One winner, nineteen stale. Same family as issue 0616 — a shared `--target-dir`
serving more than one root — one lane over: there it was `-C metadata` and a
duplicate `#[global_allocator]`, here it is a build-script SIDE EFFECT that
only fires for whoever gets there first.

## Why the 0740 stamp does not save it

`_nros_config_header_stamp` (issue 0740) does re-run — the log shows
`nano-ros: config-header stamp nros_config_generated (issue 0740)` in the
failing build. It is a `copy_if_different` of the mirrored header, so with the
mirror itself stale it correctly copies nothing. The stamp is honest about the
file it watches; the file is the problem.

## Why this is not `rm -rf`-shaped

Wiping the build dir does clear it (the first build in a fresh group re-runs
the script), and that is exactly why it will keep recurring silently: the state
is reachable from any incremental build that crosses a size change, the error
names a hash and no file, and the repair looks like "the tree was dirty".

### Confirmed by prediction, after the first prediction failed

The first prediction was that `touch`ing `packages/api/nros-{c,cpp}/build.rs`
and re-running the lane would refresh every leaf. **It did not** — the second
build left the archives at their 03:23/03:24 timestamps, untouched. That is a
SECOND missing edge and worth recording: corrosion's cargo invocation is a
ninja custom command whose declared inputs are the crate SOURCES, so touching
`build.rs` never causes ninja to invoke cargo at all. A build-script input is
invisible to the graph that decides whether the build script runs.

The diagnosis was then confirmed directly. Running one failing leaf's OWN cargo
command — verbatim from the build log, with that leaf's `CORROSION_BUILD_DIR`
and `build.rs` touched so cargo re-runs the script — rewrote the header in
place:

```
before: sz_f3c40eb64e98fb7d  2026-08-26 20:22
after:  sz_cd6bc387c5d734f9  2026-09-01 04:02:13
```

Nothing was wiped, and the leaf's own build produced the correct header the
moment its script actually ran. That is the mechanism, not build-dir rot.

## Direction

The header a fixture compiles against should not be a side effect of whether
cargo decided to run a build script. Either:

* make the corrosion copy a cmake custom command with a real dependency on the
  produced archive (the same edge `_nros_config_header_stamp` already takes for
  its own trigger — `$<TARGET_FILE:nros_c-static>`), so it re-copies per leaf
  whatever cargo does; or
* key the build script's freshness so that each `CORROSION_BUILD_DIR` is its
  own unit (a `rerun-if-env-changed` on it would do it, but see issue 0491
  before adding one on a PATH-valued variable — the value's spelling would
  become a fingerprint input).

The first is preferred: it puts the file under the build graph that consumes
it, rather than making a second thing depend on cargo's freshness opinion.

## Acceptance

* After a size-changing edit, an incremental build of the native lane links
  every leaf without a manual `touch` or a wipe.
* A leaf whose header and archive disagree still fails at LINK (do not weaken
  the anchor — it is the only reason this was caught at all).
* The failure names the stale FILE, not just the hash.

## Impact right now

This blocks any runtime measurement on an incremental native tree. Issue 0868's
runtime proof needed exactly two leaves (`examples/native/cpp/action-{client,server}/build-zenoh`)
and had to drive each one's cargo command by hand to get them. The other action
cells still fail as `Test fixture is STALE`, which is the honest report — but it
means five of ten `native_api` action tests cannot run until this is fixed or
the lane is rebuilt from scratch.
