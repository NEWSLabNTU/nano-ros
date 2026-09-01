---
id: 978
title: "The mirror prefers a leaf's OWN generated header whenever it is PRESENT,
  so every leaf but one compiles against a museum copy after any size change"
status: resolved
type: bug
area: cmake, build
related: [issue-0805, issue-0369, issue-0616, issue-0740, issue-0491, phase-340]
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
exist keep their previous timestamps looking perfectly serviceable.

## This is the size anchor working, not misfiring

`nros_config_variant_sz_<hash>` is issue 0369's anchor: an FNV over the
`(name, value)` size pairs the generated header ships
(`variant_suffix_from_sizes`). Compiling against one set of sizes and linking an
archive built with another is meant to be a LINK error rather than a silent
`_opaque` overflow — the whole 0088/0114/0122/0123/0245/0268 family.

It caught a real disagreement. In `examples/native/cpp/listener/build-zenoh`:

| artifact | anchor | mtime |
| --- | --- | --- |
| `libnros_c.a` | `sz_cd6bc387c5d734f9` | 2026-09-01 03:24 |
| `libnros_cpp.a` | `sz_cd6bc387c5d734f9` | 2026-09-01 03:24 |
| all four `nros_config_generated.h` copies | `sz_f3c40eb64e98fb7d` | **2026-08-26 20:23** |

The other anchor in the same object narrows it. `nm -u` on `main.cpp.o` wants
two symbols:

```
U nros_config_variant_sz_f3c40eb64e98fb7d                              <- MISSING
U nros_cpp_config_variant_alloc_env_..._rmw_zenoh_cffi_ros_humble_std  <- present
```

The FEATURE-slug anchor matches. So this is not the feature mismatch the panic
text in `write_header_if_absent_or_verify` tells you to check ("usually this
means nros-c and nros-cpp were built with different features") — same features,
different sizes, one half refreshed.

## Root cause: "present" was treated as "current"

`scripts/build/mirror-generated-header.sh` (issue 0805) picks the source for the
mirror from two candidates the build script writes:

1. `$CORROSION_BUILD_DIR/<name>` — this leaf's own cmake binary dir;
2. `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/<name>` — leaf-independent.

0805 preferred (1) and fell back to (2) only when (1) was **absent**, on the
stated grounds that they are "the same bytes: both are written from one
`build.rs` run".

That premise holds only for a (1) written by the same run as the current (2).
Since phase-340 the cargo `--target-dir` is shared — `<leaf>/build-*/cargo` is a
symlink to `build/corrosion-cargo/native/<hash>` — so cargo runs the build
script **once per (crate, feature set)**, not once per leaf. Every leaf after
the first keeps ITS (1) from whenever it last ran the script: present, and
arbitrarily old. The fallback never fires, and the leaf mirrors a museum header
against an archive the shared build has since rebuilt.

0805 fixed the case where a leaf has NO header. This is the case where it has a
STALE one, and it fails the other way — no missing-file error, just a wrong
answer that surfaces one layer down as a link error naming a hash and no file.

## Measured

Survey across the 20 native leaves after a size change landed:

```
 1 x sz_cd6bc387c5d734f9   2026-09-01 03:23   examples/native/c/talker/build-zenoh/...
19 x sz_f3c40eb64e98fb7d   2026-08-26 20:21   (everything else)
```

One winner, nineteen stale.

**This recurs on ordinary work, not on rare events.** Three size generations
inside six days on this tree:

| when | `EXECUTOR_SIZE` | `SUBSCRIBER_SIZE` | anchor |
| --- | --- | --- | --- |
| 2026-08-26 | 89680 | 560 | `sz_f3c40eb64e98fb7d` |
| 2026-09-01 03:23 | 89816 | 560 | `sz_cd6bc387c5d734f9` |
| 2026-09-01 16:54 | 89816 | 584 | `sz_9a3e918900c9d46d` |

The last move is phase-403 / #0896 sizing subscriptions from the message bound —
normal work. Every such landing strands every leaf but one.

## Isolated, without wiping anything

Two leaves in the same group with identical crate + features
(`examples/native/c/{custom-msg,logging}/build-zenoh`, both
`--features=ros-humble,cffi-zenoh-cffi,std,platform-posix,panic-platform`):

```
BEFORE   A=sz_f3c40eb64e98fb7d  B=sz_f3c40eb64e98fb7d
step 1: touch build.rs, run A's cargo
         A=sz_9a3e918900c9d46d  B=sz_f3c40eb64e98fb7d
step 2: NO touch, run B's cargo   -> "Finished in 0.05s" (script not re-run)
         A=sz_9a3e918900c9d46d  B=sz_f3c40eb64e98fb7d
```

One run of the script, one refreshed leaf, one stale leaf sharing its archive.

### A prediction that failed, and why it is worth recording

The first prediction was that `touch`ing `packages/api/nros-{c,cpp}/build.rs`
and re-running the lane would refresh every leaf. **It did not** — the second
build left the archives at their 03:23/03:24 timestamps, untouched. That is a
second missing edge: corrosion's cargo invocation is a ninja custom command
whose declared inputs are the crate SOURCES, so touching `build.rs` never causes
ninja to invoke cargo at all. A build-script input is invisible to the graph
that decides whether the build script runs.

## Fix

`mirror-generated-header.sh` now prefers **(2)**, the leaf-independent copy, and
falls back to (1). (2) is written by `write_header_to_target_dir` in the same
run that writes (1), and is refreshed by ANY leaf's run, so it is always at
least as fresh as (1) — never staler. (1) survives as the fallback for a build
with no resolvable cargo target dir, where (2) is never written at all.

Deliberately NOT chosen: `rerun-if-env-changed=CORROSION_BUILD_DIR` to make the
script run per leaf. That is what 0805 removed, and it is the issue-0491
path-variable fingerprint that cost 459 s → 9 s of cargo time.

Verified on a leaf that was stale, with no touch and no wipe:

```
BEFORE  mirror=sz_f3c40eb64e98fb7d   shared=sz_9a3e918900c9d46d
$ ninja -C examples/native/c/logging/build-zenoh
AFTER   mirror=sz_9a3e918900c9d46d   (and c_logging linked)
```

## Gate

`just check mirror-header-precedence` → `mirror-generated-header.sh
--self-test`, on the fast line (temp dirs only; no cargo, no cmake). Five cases:
the stale-leaf regression, 0805's original absent-leaf case, the no-shared-copy
case, the neither-exists error, and `copy_if_different` mtime preservation.

Proven non-vacuous: restoring 0805's precedence fails exactly one case —

```
FAIL a stale leaf copy loses to the shared one: got STALE
ok   a leaf with no copy uses the shared one
ok   with no shared copy the leaf copy is used
ok   no header anywhere is an error naming both paths
ok   an unchanged header does not re-stamp the dest
```

— so it catches this bug specifically rather than by being broadly strict.

## Left open

**The link error still names a hash and no file.** Acceptance item 3 of the
original filing ("the failure names the stale FILE") is NOT done: that would
mean teaching an anchor mismatch to point at the header that declared the
symbol, and the message comes from ld, which knows nothing about either. The
gate prevents the state rather than explaining it, which is the better half —
but if this class arrives through some other path, the message will be exactly
as opaque as it was here.
