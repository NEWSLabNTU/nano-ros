---
id: 1461
title: "The staleness probe exempts a regenerated-in-place header but not the
  `.stamp` beside it, so a rebuild that changes nothing marks unrelated fixtures
  STALE — and the C pubsub coordinates have produced no runtime result for 18
  days behind that verdict"
status: open
type: bug
area: testing, build
severity: medium
related: [0442, 0445, 0196, 0834, 0088, 0222]
---

## Symptom

Three of the ten `native_example_pubsub_e2e` cases refuse to launch:

```
FAIL case_2_c_zenoh   FAIL case_5_c_cyclone   FAIL case_8_c_xrce

c talker fixture not built: Test fixture is STALE — a source is newer than the
built binary:
  binary: examples/native/c/talker/build-cyclonedds/c_talker
  newer:  .../nros-c-generated/nros/nros_config_generated.h.stamp
```

Nothing ran. The C++ and Rust arms of the same suite pass on the same build.

## Cause of the `.stamp` half — fixed here

Measured on the cyclonedds leaf after a rebuild that touched only C++ headers:

```
2026-09-21 00:46  nros_config_generated.h        <- unchanged
2026-09-21 16:03  nros_config_generated.h.stamp  <- moved
```

The emitter writes the `.h` only when its content changes and touches the
`.stamp` on every build. That is exactly the shape `REGENERATED_INPLACE_HEADERS`
exists for — cbindgen output whose "newer than my binary" says nothing about the
fixture's inputs — one directory down and per-build rather than in-tree. The
exemption list names three headers by path and no stamps, so the stamp reads as
an edit event.

This is issue 0196's shape again: an exemption list narrower than the rule it
encodes.

**The fix carries a guard that is not a formality.** A `.stamp` is exempt only
while the `.h` it stamps exists beside it. A `.stamp` with NO `.h` is issue
0834's absorbing state — cargo is up to date, so the build script never re-emits
the byproduct, the POST_BUILD copy has nothing to copy, and ninja records
success forever. Exempting that would hide the one signal a developer gets. Both
directions are in the selftest.

## The other half is NOT fixed, and it is older than it looks

With the stamp exempted, the zenoh leaf names the **header itself**:

```
2026-09-21 00:37  examples/native/c/talker/build-zenoh/c_talker
2026-09-21 15:56  .../build-zenoh/.../nros_config_generated.h
```

The fixture build rewrote the header and did not relink the binary that includes
it. Either the header's content genuinely changed and the dependency edge is
missing (the 0088/0475 family — a consuming target with no edge to a generated
input), or the emitter rewrites it unconditionally in this build dir while
write-if-changed holds in the cyclonedds one. The two leaves behave differently,
which is the first thing to explain.

### Which of the two — measured 2026-09-25

**Neither, quite: the content genuinely changes, and the consuming binary's
dependency edges are CORRECT.** The file that moves is not one of its inputs.

The leaf's own `build.ninja` runs cargo twice into ONE `--target-dir`:

```
--features=ros-humble,cffi-zenoh-cffi,std,platform-posix,panic-platform --package nros-c
--features=ros-humble,rmw-zenoh-cffi,std,platform-posix,panic-platform --package nros-cpp
```

`nros-cpp`'s `rmw-zenoh-cffi` forwards `nros-c/rmw-zenoh`; the C side names
`cffi-zenoh-cffi`, a back-compat ALIAS for that same feature
(`cffi-zenoh-cffi = ["rmw-zenoh"]`). Cargo keys a unit by its feature SET, so
the alias buys the directory a SECOND `nros-c` unit. Both write
`<target-dir>/nros-c-generated/nros/nros_config_generated.h`, and their two
headers differ in exactly one line:

```
$ diff <(grep -v NROS_CONFIG_VARIANT unit-4918.h) <(grep -v NROS_CONFIG_VARIANT unit-2ff5.h)
IDENTICAL apart from the VARIANT line
116c116
< #define NROS_CONFIG_VARIANT "alloc_env_..._rmw_cffi_rmw_zenoh_ros_humble_std"
> #define NROS_CONFIG_VARIANT "alloc_cffi_zenoh_cffi_env_..._rmw_cffi_rmw_zenoh_ros_humble_std"
```

Every probed size agrees, so the size ANCHOR is the same symbol in both
(`nros_config_variant_sz_79e49f14b7cab45c` — issue 0369 derives it from the
values, not the feature spelling). `write_atomic` compares BYTES, so each build
rewrites what the other just wrote: the mtime moves every time, by
construction, and no rebuild can settle it.

**Why the two leaves differ** is the same table, one column over:

| leaf | `--package nros-c` | `--package nros-cpp` | nros-c resolved |
| --- | --- | --- | --- |
| cyclonedds | `rmw-cffi` | `rmw-cffi` | ONE set — header frozen, only the `.stamp` moves |
| zenoh | `cffi-zenoh-cffi` | `rmw-zenoh-cffi` | TWO sets — header oscillates |
| xrce | `cffi-xrce-c` | `rmw-xrce-cffi` | TWO sets — header oscillates |

Matches the mtimes exactly: cyclone's header last moved at 10:30 with its stamp
at 16:03; zenoh's and xrce's headers both moved at 15:56.

**Why no relink, and why that is right.** `c_talker` reaches the config header
through a mirror, not through that path: `mirror-generated-header.sh` copies the
newest shared candidate into the crate's own `include/` under `restat = 1`, and
the TU depends on a per-target `copy_if_different` stamp
(`_nros_cfg_stamp/c_talker/nros_config_generated.stamp`,
`_nros_config_header_stamp`). `ninja -t deps` confirms it: the object's recorded
inputs name the mirrored include, never the shared path. So the mirror sees the
same bytes it already has, nothing propagates, and the binary correctly stays
put. **The binary was never stale.**

What put the shared path in front of the probe is `libnros_cpp.d` — the
staticlib's own cargo dep-info, which lists both the header and its `.stamp`,
because `write_header_if_absent_or_verify` declares them `rerun-if-changed` (the
issue-0834 self-heal). `cargo_rust_inputs` reads that `.d`, so an oscillating
BYPRODUCT arrives at the probe wearing the clothes of an edited SOURCE.

This is issue 1354's "What is NOT fixed" — one directory serving two feature
sets — in a cmake leaf rather than a check lane, and it is the same defect
issues 1100 and 1156 fixed at their own sites. 1100's split (`alloc` +
`param-services`) changed the SIZES and killed the build; this one changes a
human-readable slug and kills nothing, which is why it ran for 18 days.

The probe's own absorbing-verdict counter (issue 0445) says how long this has
been true:

```
NOT RUN: 8th consecutive stale verdict for this fixture, first 18d ago.
This coordinate has produced no runtime result since then — whatever it
would have done is being absorbed by this message (issue 0445). If the
rebuild does not clear it, suspect the probe before trusting the verdict.
```

**Eighteen days and eight verdicts.** Every C pubsub coordinate has been
reporting a message instead of a result since then, and the counter that says so
is exactly the mechanism issue 0445 added for this case. It worked; nobody read
it.

## Repro

```sh
just build-test-fixtures lane=native
cargo nextest run -p nros-tests --cargo-profile nros-relwithdebinfo \
  -E 'binary(native_example_pubsub_e2e)' --no-fail-fast
stat -c '%y %n' examples/native/c/talker/build-zenoh/c_talker \
  examples/native/c/talker/build-zenoh/cargo/*/nros-c-generated/nros/nros_config_generated.h
```

## Acceptance

* A config-header `.stamp` beside its header is not an edit event; without the
  header it still is. **Done** — `Exemption::ConfigHeaderStamp`, both directions
  in the selftest, and the exemption is reported in the probe's accounting line
  so a reader can see it was applied.
* The C talker leaves relink when their generated config header changes, or the
  header stops being rewritten when its content has not changed. Which of the
  two is the defect is answered by diffing the emitted header against the
  previous content, not by rebuilding until it passes. **Done, and it was
  neither** — see "Which of the two" above. The header's content really changes
  and the binary's edges are correct; what was wrong is that TWO `nros-c`
  feature sets share the path, for no reason but a spelling.
* The three C pubsub cases produce a runtime result.
* Whatever those cases have been hiding for 18 days is stated, because "it was
  stale" is not a test result.

## Fix — 2026-09-25

**1. The two roads name the same feature.** `c_cffi_feature` in the zenoh and
xrce descriptors is `rmw-zenoh` / `rmw-xrce` — what `nros-cpp` forwards — not
the `cffi-*` alias. Swept across every road that builds both crates into one
target dir, because this had already been fixed at exactly one site and left at
the others (`zephyr/CMakeLists.txt`'s zenoh branch carried the correct spelling
AND the reasoning for it, while the xrce branch beside it and the C-API branch
above it did not):

```sh
git grep -n 'cffi-zenoh-cffi\|cffi-xrce-c\|cffi-dds-cffi' -- ':!docs' ':!*.md'
```

sites: both `nros-rmw.toml` descriptors, `zephyr/CMakeLists.txt` (3),
`integrations/nuttx/Makefile` (2), the two NuttX board FFI manifests,
`size_probe_verify.sh`. The aliases stay in `nros-c`'s manifest — they exist for
out-of-tree glue that still passes the old name.

**2. The owner's write uses the same equality its verifier uses.**
`write_header_to_target_dir` no longer rewrites the shared header for a
difference confined to `NROS_CONFIG_VARIANT`, which `defines_of` already
excludes from the mismatch check and which no consumer reads. It says so with a
`cargo:warning` naming both variants rather than smoothing it over, because two
feature sets sharing the path is still 1354's unfixed half wherever it happens.
A disagreeing `#define` is still written — that is the sizes contract.

**3. A gate for the class.** `check-nros-c-feature-agreement` (fast line):
`c_cffi_feature`'s closure must equal what `nros-cpp`'s `<cargo_feature>-cffi`
forwards, and no in-tree glue may name a back-compat alias. Both rules are
DERIVED from the two manifests — nothing enumerates a feature — and the alias
set is derived too, which is what keeps `rmw-cyclonedds = ["rmw-cffi"]` (a name
both roads already agree on) out of it. Run against the pre-fix tree it names
all five sites; the first draft's pathspec globs matched `zephyr/**/*.txt` to
nothing and reported only two, which is why its reach is now asserted
(`REACH_MUST_HOLD`).

## What the three C cases were hiding

Not known yet, and it cannot be known until they run — that is what an
absorbing verdict costs. What is now known is the WINDOW: the coordinate's last
runtime result predates the first of the eight verdicts, 18 days back, so any
regression landing in `examples/native/c/talker`, the C pubsub path or the
zenoh C road in that window has never been observed by this suite. The counter
(issue 0445) said so on every run.
