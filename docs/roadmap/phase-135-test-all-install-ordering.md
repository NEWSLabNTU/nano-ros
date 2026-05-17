# Phase 135 — `test-all` Install-Ordering Bug

**Goal.** Make `just test-all` (and therefore `just ci`) self-contained:
populate `build/install/` BEFORE running the nextest tests that build
C / C++ examples via `cmake … find_package(NanoRos)`. Today the
install happens at the end (inside `_test-c-codegen` →
`tests/c-msg-gen-tests.sh:56` → `just install-local`), so the first
clean `just ci` on a fresh checkout fails ~58 native_api tests and
~24 dds_api tests because `build/install/lib/cmake/NanoRos/` does not
yet exist.

**Status.** Not started.

**Priority.** P2 — broken first-run CI on fresh clones. Warm
machines that already ran `just install-local` (or that previously
ran `just ci` to completion and let `_test-c-codegen` populate the
install) pass on retry.

**Depends on.** None.

**Related.** Phase 133 (sweep), Phase 134 (UDP multicast — the other
class of native_api failures; these two phases together close the
post-Phase-131 ci gap).

---

## Overview

`test-all` in `justfile:503` declares only `build-zenohd` as a
dependency. The recipe body then runs `cargo nextest run`. Tests
under `packages/testing/nros-tests/tests/native_api.rs` call
`build_c_example("native/c/zenoh/talker", …)` which runs
`cmake -DCMAKE_PREFIX_PATH=…/build/install ..`. Without a prior
`just install-local`, the install directory has only stale (or
zero) `lib/cmake/NanoRos/` and the configure step fails with:

```
CMake Error at .../CMakeFindDependencyMacro.cmake:93 (find_package):
  Could not find a package configuration file provided by "NrosPlatformPosix"
  with any of the following names:
    NrosPlatformPosixConfig.cmake
    nrosplatformposix-config.cmake
```

Same shape for `NanoRos`, `NrosRmwZenoh`, `NrosRmwDds`,
`NrosPlatformThreadx`, etc. — every cmake-driven test fixture
expects the install layout to already exist.

The install eventually happens late in test-all:

```
test-all
  ├── nextest run                       ← native_api / dds_api fail here
  ├── test-doc                          ← passes
  ├── test-miri                         ← passes
  └── native _test-c-codegen
        ├── test_generate_c             ← passes
        └── tests/c-msg-gen-tests.sh    ← calls `just install-local`
                                          (the install everyone needs)
```

So `_test-c-codegen` populates the install at the end, and the
NEXT `just ci` invocation passes (because the install is now fresh).
Cold-start CI fails.

---

## Architecture

Two viable fixes:

### A. Hoist `install-local` to a test-all dependency (preferred)

```diff
-test-all verbose="": build-zenohd
+test-all verbose="": build-zenohd install-local
```

`install-local` is idempotent (the underlying cmake `install(…)`
rules write into `build/install/`). Re-running it once at the top of
test-all costs the same as letting `_test-c-codegen` do it later —
just earlier in the pipeline.

Downsides: `install-local` is heavy (cyclonedds + 5 platform crates
+ 3 RMW crates). Adds ~30 s to a warm `just ci`. Acceptable.

### B. Move install into the test fixture

`packages/testing/nros-tests/src/fixtures/binaries/mod.rs::build_c_example`
already gates on `build_nros_c_lib()` (a `OnceCell` over `cargo
build -p nros-c --release`). Add a second OnceCell-gated step that
shells out to `just install-local` on first call. Less ergonomic
(test fixture invoking just), but keeps the "tests do what they
need" pattern.

Downsides: spawning `just install-local` from inside a test process
inverts the layering and risks deadlocks if the fixture itself is
inside a `just install-local` subprocess. The dep-arrow in option A
is the cleaner choice.

### C. Document `install-local` as a manual prereq (rejected)

Adds friction. CLAUDE.md already says "Always `just ci` after task" —
making it actually self-sufficient matches the documented contract.

---

## Work Items

- [ ] 135.1 — Add `install-local` as a `test-all` dependency in
      `justfile:503`. Verify it runs ahead of `cargo nextest run`.
      **Files.** `justfile`.

- [ ] 135.2 — Sanity-check the warm-machine timing: `just ci` on a
      machine that already has install populated should add at most
      ~5 s (the cost of re-running idempotent install steps), not the
      full ~30 s of a cold install.
      **Files.** none.

- [ ] 135.3 — Confirm `tests/c-msg-gen-tests.sh` still works when
      install is already current at script start (it currently
      re-runs `just install-local` unconditionally on line 56). Either
      remove the redundant call from the script (now that test-all
      guarantees install), or leave it for the standalone-script path
      (someone running the c-msg-gen script directly without
      test-all).
      **Files.** `tests/c-msg-gen-tests.sh`.

- [ ] 135.4 — Re-run `just ci` from a checkout where `build/install/`
      has been wiped (`rm -rf build/install/`). Expected: install
      step runs first, then nextest, then doctest / miri / c-codegen
      — all green (modulo Phase 134's UDP multicast linker class +
      env-precondition `[SKIPPED]` panics).

---

## Acceptance

- [ ] `rm -rf build/install && just ci` does NOT fail with
      `Could not find a package configuration file provided by "NrosPlatformPosix"`.
- [ ] First-run cold `just ci` matches second-run warm `just ci` in
      pass/fail counts (modulo unrelated flakes).
- [ ] `tests/c-msg-gen-tests.sh` standalone invocation still works.

---

## Notes

- Hoisting install to a dep does NOT fix Phase 134's linker error —
  that's a separate root cause (mismatched zenoh-pico feature gates
  inside `libnros_rmw_zenoh.a`). After both 134 and 135 land,
  cold-start `just ci` failures should be limited to genuine
  `[SKIPPED]` env preconditions (XRCE agent, ROS 2 humble,
  cross-toolchain) plus actual code bugs.
- Phase 131 didn't change `test-all` or any install recipe — it
  just forced this latent issue into the open by being the first
  branch in a while to trigger a full clean CI run on this machine.
