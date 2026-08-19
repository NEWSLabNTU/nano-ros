---
id: 696
title: "Every native C/C++ fixture reads STALE against `nros-tests/src/lib.rs` — a file in none of their dep graphs, so no build can clear it"
status: resolved
type: bug
severity: high
area: testing, build
related: [issue-0445, issue-0442, issue-0391, issue-0196]
---

## Symptom

After a `git pull --rebase` that rewrote `packages/testing/nros-tests/src/lib.rs`,
15 native C and C++ tests fail in `just ci` — all in well under a second, which
is the tell that they never ran anything:

```
FAIL [ 0.082s] nros-tests::native_example_pubsub_e2e native_example_pubsub::case_2_c_zenoh
FAIL [ 0.090s] nros-tests::native_example_pubsub_e2e native_example_pubsub::case_3_cpp_zenoh
FAIL [ 0.088s] nros-tests::native_example_reqresp_e2e native_example_reqresp::case_02_c_zenoh_service
… test_service_callback_interop_*, test_native_talker_starts, safety_e2e C/C++ …
```

Each is the absorbing STALE verdict:

```
[SKIPPED] c talker fixture not built: Build failed: Test fixture is STALE — a source is newer than the built binary:
  binary: examples/native/c/talker/build-zenoh/c_talker
  newer:  packages/testing/nros-tests/src/lib.rs
  probe:  examined 352 input(s); exempted 33 regenerated-in-place header + 0 cargo OUT_DIR product
  NOT RUN: 4th consecutive stale verdict for this fixture, first 2m ago.
```

## The named file is not an input to that binary

Checked every place the fixture's dependencies are recorded, and it is in none
of them:

```
$ grep -c 'nros-tests' examples/native/c/talker/build-zenoh/cargo/build/x86_64-unknown-linux-gnu/release/libnros_c.d
0
$ ninja -C examples/native/c/talker/build-zenoh -t deps | grep -c 'nros-tests'
0
$ grep -rl 'nros-tests/src/lib.rs' examples/native/c/talker/build-zenoh
(nothing)
```

`nros-tests` is the TEST HARNESS crate. `examples/native/c/talker` is a CMake C
executable linking `libnros_c.a`; nothing in that chain compiles the harness.
The verdict names a file the binary has never depended on.

## And no build can clear it

This is the part that makes it a blocker rather than a nuisance:

```
$ just native build-c        # exit 0
$ ls -la --time-style=+%H:%M:%S examples/native/c/talker/build-zenoh/c_talker
-rwxr-xr-x 1 aeon users 2758704 12:40:30 …/c_talker      # unchanged
```

CMake is right to do nothing — the harness is not in its graph — so the binary
keeps its old mtime and the probe returns the same verdict forever. The
remedies the message and CLAUDE.md offer (`just build-test-fixtures`, rebuild
the family) all terminate without touching the artifact. Only deleting the
build tree, or a rebuild forced by some *other* input changing, would clear it,
and neither is what the verdict asks for.

The verdict's own counter saw this coming — "4th consecutive stale verdict for
this fixture … If the rebuild does not clear it, suspect the probe before
trusting the verdict" (issue 0445). That instrumentation worked exactly as
designed; this issue is the case it was built to surface.

## Impact

Any host that pulls a commit touching `packages/testing/nros-tests/src/` has 15
red tests in tier 1 that no documented action fixes. The pull does not even have
to change behaviour — git rewriting the file's mtime is enough, which is the
fixture mtime treadmill CLAUDE.md already warns about, except that here the
treadmill has no exit.

## Not yet identified

WHERE the probe gets the path. It is not `cmake_dep_info_newer_source`
(`ninja -t deps` has no such entry), not the cargo `.d` (no entry, and the
executable has no `.d` at all), and not `zpico_recorded_inputs` (its recorded
entries are `build.rs`, `version.rs`, `tests/c_stubs/*`, `../nros-rmw-abi/include/nros`
and two cbindgen registry dirs). Some arm contributing to the 352 examined
inputs is reaching the harness crate; finding which is step one.

Worth checking as part of that: whether an arm resolves a RELATIVE
`rerun-if-changed` entry (several are recorded relative — `build.rs`,
`src/gen/build.rs`, `version.rs`) against the wrong base directory, since
`newest_path_after` treats a directory as "anything under it" and a
mis-resolved base would drag in an unrelated subtree.

## Workaround

`NROS_SKIP_FIXTURE_CHECK=1` for the run, having confirmed by other means that
the fixtures are current. That is the bypass the message names, and it disables
the gate for every fixture, not just the misjudged ones.


## RESOLVED 2026-08-19 — a relative `rerun-if-changed` resolved against the test process's CWD

"Not yet identified: WHERE the probe gets the path" — it is
`zpico_recorded_inputs`, and the hypothesis this issue offered was right: a
RELATIVE entry resolved against the wrong base. The base was not a
mis-constructed directory in the code; it was the ABSENT one.

```rust
let path = PathBuf::from(rest.trim());
let Ok(path) = path.canonicalize() else { continue };   // <- relative => CWD
if path.starts_with(&root) { out.push(path) }
```

`Path::canonicalize` resolves a relative path against the process's current
directory, and a nextest binary runs with CWD = `packages/testing/nros-tests`.
`zpico-sys`'s build script records `cargo:rerun-if-changed=src/lib.rs` — meaning
its OWN `src/lib.rs` — so the entry resolved to the HARNESS's `src/lib.rs`,
passed the in-repo filter (it genuinely is in-repo), and entered the input set of
every native C/C++ fixture.

### Why exactly one wrong file, always the same one

The output records 18 distinct relative entries. From that CWD, `src/lib.rs` is
the **only** one that resolves to something real — `cbindgen.toml`,
`c/zpico/zpico.c`, `c/platform/errno_override.h` and the rest fail to
canonicalize and are silently skipped. So the probe simultaneously watched one
file it must not and missed ten it should: the zpico C shim sources this arm
exists to cover were never in the set.

### Fix

Resolve a relative entry against the crate that RECORDED it. `zpico_manifest_dir()`
is spelled once and shared with the bootstrap walk below it, so the two arms
cannot disagree about where that crate is.

Verified, with `nros-tests/src/lib.rs` freshly touched — the exact pull shape
that produced the report:

| | |
| --- | --- |
| `case_2_c_zenoh`, `case_3_cpp_zenoh` | **pass** (3.3 s — they run) |
| same, with the fix reverted | FAIL in 0.101 s, `newer: …/nros-tests/src/lib.rs` |

The mutation reproduces the reported verdict verbatim, so the fix is what
changed the outcome rather than some rebuild in between. A unit test
(`relative_recorded_input_resolves_against_the_recording_crate`) pins the base
and asserts both files exist first, so it cannot pass vacuously.

### A note on how this was ruled out before

This issue lists `zpico_recorded_inputs` under "not it", on the evidence that its
recorded entries are `build.rs`, `version.rs`, `tests/c_stubs/*` and so on. That
inspected WHAT was recorded, not HOW it was resolved — the two differ precisely
when a path is relative. Reading structure instead of measuring the artifact is
the same class of error as the museum-binary reading in #0678.

### Not this issue

A Rust fixture still reads STALE against
`examples/native/rust/talker/generated/builtin_interfaces/src/msg/duration.rs`.
That file IS in that binary's dependency graph and a rebuild clears it — an
ordinary treadmill entry, not the unclearable verdict this issue is about.
