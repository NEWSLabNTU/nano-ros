---
id: 720
title: "A fixture binary name that no CMake target produces reads as `fixture missing`, so the test skipped forever"
status: resolved
type: bug
severity: high
area: testing
related: [issue-0692, phase-369]
resolved_in: "2026-08-20 — `check-fixture-binary-names`"
---

# 0720 — a wrong artifact name does not fail a test, it silences one

A test asks a resolver for an artifact **by name**:

```rust
build_threadx_rv64_rust_example_rmw("listener", "riscv64_threadx_rust_listener", Rmw::Cyclonedds)
```

The resolver joins that name onto the leaf's build directory
(`<leaf>/build-<rmw>/<binary_name>`). If the name is wrong the path does not
exist, and the call site's `unwrap_or_else` turns that into:

```rust
nros_tests::skip!("rust cyclone listener fixture missing (just threadx_riscv64 build-fixtures): {e:?}")
```

— a message that explains itself and blames the build. A name no target produces
therefore reads as *"nobody built the fixtures"*, forever, on a tree where they
were all built minutes earlier.

## What happened

phase-369 W4 (`7c455016f`, 2026-08-20) renamed the threadx-rv64 rust leaves'
CMake targets to be RMW-neutral, for a good reason: the zenoh build directory
was emitting an ELF named `riscv64_threadx_rust_talker_cyclonedds`, because the
leaf hardcoded the suffix into its target name.

```
-project(riscv64_threadx_rust_listener_cyclonedds VERSION 0.1.0 …)
+project(riscv64_threadx_rust_listener VERSION 0.1.0 …)
```

The rename updated the talker's test site and missed the listener's — the only
other consumer, one line in one file:

```
$ grep -rn riscv64_threadx_rust_listener . | grep -v /build-
examples/qemu-riscv64-threadx/rust/listener/CMakeLists.txt:2:project(riscv64_threadx_rust_listener …)
examples/qemu-riscv64-threadx/rust/listener/CMakeLists.txt:58:nros_threadx_rv64_rust_app(riscv64_threadx_rust_listener
packages/testing/nros-tests/tests/threadx_riscv64_qemu.rs:263:        "riscv64_threadx_rust_listener_cyclonedds",
```

From then on `test_threadx_riscv64_cyclonedds_two_qemu_rust_pubsub` skipped
every run.

## Why that mattered more than one skip

That test is the **only** consumer of the rust CycloneDDS image — the image
issue 0692 was about. So while 0692 was being investigated, resolved, and
archived, the suite reporting on it was:

```
Summary [1.366s] 5 tests run: 4 passed, 1 failed, 0 skipped
All failures were [SKIPPED] preconditions — treating as pass.
```

`check-skip-budget` did see it and printed `1 skip(s) — capability=1`. That is a
count, not a name, so nothing connected it to the image under investigation.
This is the "STALE verdict is ABSORBING" hazard (issue 0445) one level over: the
non-result replaces the result with a message about itself.

After the fix, on the same tree: `5 tests run: 5 passed, 0 skipped`.

## Fix

The test's binary name corrected to `riscv64_threadx_rust_listener`. Swept for
siblings — the expected-vs-declared diff over the whole platform found exactly
this one:

```
$ grep -oh '"riscv64_threadx[a-z0-9_]*"' packages/testing/nros-tests/tests/*.rs | tr -d '"' | sort -u > want
$ grep -rhoP 'project\(\Kriscv64_threadx[a-z0-9_]*' examples/qemu-riscv64-threadx/*/*/CMakeLists.txt | sort -u > have
$ comm -23 want have
riscv64_threadx_rust_listener_cyclonedds
```

Gated by `check-fixture-binary-names` (`scripts/check-fixture-binary-names.py`,
on the `just check` fast line): every string literal handed to a cmake-leaf
fixture resolver as its `binary_name` must be declared as a target in that
leaf's `CMakeLists.txt`. It is **static**, so it fails on the rename commit
rather than on the next full sweep.

Nine of the seventeen call sites pass literals. The other eight are wrappers
taking `case`/`binary` as parameters, fed from `matrix::CELLS` through small
`&'static str` mappers; the gate reports them as `8 non-literal call(s) not
checked` rather than passing over them silently, so a refactor into variables
cannot quietly empty it. Those eight names (`c_talker`, `c_listener`,
`cpp_talker`, `cpp_listener`) were verified by hand against
`examples/native/{c,cpp}/{talker,listener}/CMakeLists.txt` and are correct.

## How it surfaced

While confirming issue 0692's resolution on a clean rebuild of the
threadx-riscv64 family: the C and C++ pubsub pairs passed, which proves QEMU and
the platform preconditions were met, so the rust pair's "capability" skip could
not be environmental.
