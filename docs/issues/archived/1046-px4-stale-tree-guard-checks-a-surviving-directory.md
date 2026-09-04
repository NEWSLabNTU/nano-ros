---
id: 1046
title: "The PX4 SITL stale-tree guard asserts a module DIRECTORY, which outlives the build that linked it — so it passes on exactly the tree it exists to reject"
status: resolved
type: bug
area: testing
severity: medium
found: 2026-09-04
resolved: 2026-09-05
resolved_in: 0a47f949a (guard) + phase-424 (the archive/header pairing)
related: [phase-325, phase-424, 0196, 0360, 0445, 0859, 1050]
---

# A guard that diagnoses the problem in prose and cannot detect it

## The guard

`packages/testing/nros-px4-sitl-test/tests/px4_uorb_interop_e2e.rs:65-75`:

```rust
let module_dir = build_dir.join("external_modules/modules/nros_uorb_demo");
assert!(
    module_dir.is_dir(),
    "SITL tree at {} was built WITHOUT the uORB interop example (no {}).\n\
     PX4 takes one EXTERNAL_MODULES_LOCATION per build and they share this \
     build dir, so another root (e.g. `just px4 build-sitl-cpp`) built last.\n\
     Rebuild with:\n    just px4 build-sitl-example",
    ...
);
```

Its message is a correct and complete diagnosis of the failure mode. Its
assertion cannot observe that failure mode.

## Measured 2026-09-04

On a tree whose last SITL build was `just px4 build-bridge-example` (the
phase-325 W3.3 work), i.e. exactly the "another root built last" case the
message describes:

| module | module dir | `bin/px4-<mod>` shim | occurrences in `bin/px4` |
| --- | --- | --- | ---: |
| `nros_uorb_demo` | present | present | **0** |
| `nros_uorb_bridge` | present | present | 8 |

Read with:

    strings -a build/px4_sitl_default/bin/px4 | grep -c nros_uorb_demo

**Both the module directories and the `bin/px4-<mod>` shims survive across
builds.** Only the last root's modules are actually linked into `bin/px4`. So
the guard passes, the test proceeds, and it dies at `nros_uorb_demo start` with
a command-not-found — which is precisely the confusing failure the guard was
written to replace with a clear one.

The guard checks an artifact that outlives the thing it is a proxy for.

## Why this is the repo's own named class

CLAUDE.md, issue 0196: *"Build-side stale probes must watch the same inputs as
test-side gates — a probe that misses `generated/**` lets a museum binary pass
every sweep."* Same shape, different input: here the probe watches a directory
while the claim is about a **binary's contents**.

It is also the 0445 shape — a verdict that is absorbing because the thing it
reports on never ran — and the 0859-0862 shape, where a stale artifact produced
a confident wrong answer. The distinguishing feature in all of them is a check
that cannot tell *present* from *current*.

## FIXED 2026-09-04 — and verified in both directions

`px4_uorb_interop_e2e.rs` now scans `bin/px4` for the module's command name.

* **Rejects the stale tree.** On the tree that exposed this (last built from
  `build-bridge-example`), the guard fails in **0.04 s** with the message naming
  the binary and the reason — where the old directory check PASSED and the test
  went on to die at `nros_uorb_demo start`.
* **Passes a correct tree.** After `just px4 build-sitl-example` (9 occurrences
  of `nros_uorb_demo` in `bin/px4`), the guard passes and the test proceeds to
  boot SITL. Its later failure at `nros_uorb_demo start: status 255` is a
  runtime matter and outside this guard's contract.

**A byte scan, not `nm`, and that choice turned out to be load-bearing.** While
verifying this, `nm --defined-only` reported ZERO `nros_cpp_` symbols in a
freshly built `libnros_cpp.a` that in fact contained them — the system binutils
(LLVM 14) cannot read rust 1.96's LLVM 22 objects and says `no symbols` for
every Rust CGU:

    nm: nros_cpp-….rcgu.o: no symbols
    bfd plugin: LLVM gold plugin has failed to create LTO module:
      Opaque pointers are only supported in -opaque-pointers mode
      (Producer: 'LLVM22.1.2-rust-1.96.0-stable' Reader: 'LLVM 14.0.0')

A byte scan found the same symbol 5 times. So a guard written with `nm` would
have been a new instance of this very issue — a check that reports absence it
cannot observe. `integrations/px4/NanoRosPx4Module.cmake` already knew: it
resolves `llvm-nm` through `rustc --print sysroot` rather than trusting the
system one.

## Original direction (kept — this is what was done)

**Assert on binary CONTENT, not directory presence.** The cheapest form is what
measured it above — the module's command name appears in `bin/px4`:

```rust
let px4 = build_dir.join("bin/px4");
let linked = std::fs::read(&px4).map(|b|
    b.windows(NAME.len()).any(|w| w == NAME.as_bytes())).unwrap_or(false);
assert!(linked, "... built WITHOUT {NAME} (its directory survives from an \
        earlier build; the BINARY does not contain it) ...");
```

A byte-scan of a ~59 MB binary is milliseconds and needs no PX4 boot. Booting
SITL and checking the command registers is stronger and much slower; the scan is
enough to turn a confusing runtime failure into the message that already exists.

**Keep the existing message.** It is right. Only the predicate is wrong.

## The three "not covered" items, now swept (2026-09-05)

* Whether `px4_xrce_e2e.rs` has the same shape. It guards
  `path.join("Tools").is_dir()` (`:42`), which is a *tree* check rather than a
  built-module check, so it is probably a different question — unverified.
* Whether anything else in the tree proxies "was X built into this artifact" by
  a directory test. Not swept.
* The `bin/px4-<mod>` shims surviving is itself worth a look: a shim for a module
  that is not in the binary is a second stale artifact, and it may be what makes
  the runtime failure read as "command exists but does nothing" rather than
  "command not found". Not investigated.

## The sweep — 2026-09-04, phase-424

The test guard was the reported site. The rule is *a check must assert something
that cannot outlive the build it reports on*, so the question is where else this
tree proxies "was X built from Y" by a survivor. Swept the PX4 integration:

| site | predicate | verdict |
| --- | --- | --- |
| `px4_uorb_interop_e2e.rs:65` module dir | `is_dir()` | the reported site; fixed above |
| `NanoRosPx4Module.cmake` generated-header dirs | `IS_DIRECTORY` | **same defect, fixed now** |
| `NanoRosPx4Module.cmake` archive resolve | `EXISTS` on the `.a` | insufficient but not wrong — see below |
| `px4_xrce_e2e.rs:42` `Tools/` | `is_dir()` | **not this class.** It asks "is the PX4 tree checked out", and a source tree is exactly the thing a directory test is right for. No build artifact stands behind it |
| `nros-px4-register-check` | — | links no nano-ros archive at all (raw `px4_add_module`, uORB sources compiled inline + a weak register fallback), so it has no artifact to be stale against |

**The second row is the same bug, one layer down, and it was live.** Measured in
the shared checkout with no build running:

    target/nros-cpp-generated/nros/nros_cpp_config_generated.h   PRESENT
        (naming nros_cpp_config_variant_..._rmw_zenoh_cffi_...)
    target/release/libnros_cpp.a                                 ABSENT

The header outlived its archive completely, and both `IS_DIRECTORY` checks
passed. That header carries storage SIZES, so the pairing it stands for is the
0268 silent-overflow class, and the only thing catching a mismatch was the issue
0360 anchor firing at LINK time — ~1100 targets and ten minutes in.

### Fix: pair CONTENT against CONTENT

`integrations/px4/NanoRosArchivePairing.cmake` reads the variant symbol *out of*
the header (rather than recomputing the slug — a second derivation is the class
`row_coord()` records) and asserts the archive defines it. Neither side can
outlive the other's build without it firing. It is exactly the anchor's
condition, evaluated at configure time instead of at link.

Both generated headers are checked, not just the C++ one: the C stamp is a SIZE
HASH (`sz_<hash>`) and the C++ one a feature slug, and they move independently.
Measured — rebuilding `nros-cpp` from `std,rmw-cffi,platform-posix` to
`std,rmw-cffi` moved the C++ slug (`..._platform_posix_rmw_cffi_std` →
`..._rmw_cffi_std`) and left the C hash at `sz_cf866c9020e05fd0`. Checking one
would have been coverage narrower than the rule, which is issue 0196.

### The `nm` trap, re-measured on this host

This issue's own warning held, and it is worth the second data point because it
is what makes the byte scan non-negotiable:

| tool | occurrences of `nros_cpp_config_variant_*` in a freshly built `libnros_cpp.a` |
| --- | ---: |
| system `nm --defined-only` | **0** |
| `<rustc sysroot>/…/llvm-nm --defined-only` | 1 |
| byte scan (`grep -ac`) | 3 |

    bfd plugin: LLVM gold plugin has failed to create LTO module:
      Opaque pointers are only supported in -opaque-pointers mode
      (Producer: 'LLVM22.1.6-rust-1.97.1-stable' Reader: 'LLVM 14.0.0')
    nm: nros_cpp-….rcgu.o: no symbols

System `nm` reports absence it cannot observe, and does not fail while doing it.
The pairing check uses `file(STRINGS)` — no binutils, no PATH, no toolchain
agreement. Cost on the 25 MB archive: **0.004 s** on a hit (`LIMIT_COUNT 1`
short-circuits), **0.243 s** on a miss (full scan). The gate forbids reaching for
`nm` here, because the mistake is invisible at the call site.

### Mutation-checked, on real artifacts

Two real archives from two real cargo invocations, then paired crosswise:

    header B + archive B                -> PASS (0.005 s)
    header A + archive B  (issue 1050)  -> FAIL, naming both variants
    header A + archive A                -> PASS

`just check px4-archive-header-pairing` runs the real cmake predicate against
fixtures in both directions on every invocation — 11 cases, 0.12 s, no PX4 tree
and no cargo build. Six exercise the predicate (match, mismatch, **surviving
generated dir with no header — this issue verbatim**, stampless header, missing
archive, header path that is a directory); five exercise the wiring rule,
including a control that prose describing the rule does not trip it.

That control is not decoration. The gate's first draft banned `IS_DIRECTORY` near
a generated path and went red on its own fix — the comment explaining the rule
contains both words. Left in, it would have taught the next person that
documenting the reasoning breaks the build. The rule was also simply the wrong
one: that loop is correct for the four SOURCE include dirs and merely
insufficient for the generated ones, so what makes them safe is the pairing
assertion existing beside it, not the loop's absence.

## Still not covered

* **No PX4 build was run.** `third-party/px4/PX4-Autopilot` is an empty,
  uninitialised gitlink on this host, so nothing here was verified through a real
  `make px4_sitl_default`. The cmake predicate is measured against real
  nano-ros artifacts; its *placement* inside a PX4 configure is not.
* The `bin/px4-<mod>` shims surviving (noted above) is still uninvestigated.
* Whether any non-PX4 lane pairs a generated header with an ambient archive the
  same way. The PX4 integration is swept; the rest of the tree is not.

Re-measured on the same host. The fix in `0a47f949a` still rejects the stale
tree — run against the build dir as it stands today (last built from
`build-bridge-example`):

```
$ cd packages/testing/nros-px4-sitl-test
$ PX4_AUTOPILOT_DIR=.../PX4-Autopilot cargo test --test px4_uorb_interop_e2e
SITL tree at .../build/px4_sitl_default was built WITHOUT the uORB interop
example: .../bin/px4 does not contain `nros_uorb_demo`.
...
test result: FAILED. 0 passed; 1 failed; finished in 0.04s
```

and the table it keys on still holds — `strings -a bin/px4 | grep -c <name>`:

| module | module dir | `bin/px4-<mod>` shim | occurrences in `bin/px4` |
| --- | --- | --- | ---: |
| `nros_uorb_demo` | present | **absent** | 0 |
| `nros_uorb_bridge` | present | present | 8 |

**One number in the original table does not reproduce: the demo's shim is now
absent.** So the shims are *not* reliably stale — PX4 cleans some of them. That
makes the third item below a non-finding rather than a second stale artifact:
the shim is inconsistent, which is a reason not to key a guard on it, and the
guard no longer does. The module DIRECTORY, which is the half the guard used to
key on, does survive — that claim reproduces exactly.

* **`px4_xrce_e2e.rs` — different question, confirmed.** Its
  `path.join("Tools").is_dir()` (`:42`) asks "is this a PX4 checkout", which is
  what it says and is correct for it. It needs no built-module guard because it
  BUILDS SITL itself (`build_vanilla_sitl()`, `:50`). That is worth its own
  look for two unrelated reasons — it is a `make` inside a test (CLAUDE.md's
  "no compilation inside tests"), and PX4's `cmake-cache-check` only compares
  options it was PASSED, so a `make px4_sitl_default` with no
  `EXTERNAL_MODULES_LOCATION` reconfigures nothing and inherits the cached one,
  i.e. "vanilla" is not vanilla. Neither affects this issue: an extra module in
  the tree does not break the XRCE path. Not filed here.
* **Nothing else proxies "was X built into this artifact" by a directory test.**
  Swept the whole PX4 seam: `nros_px4_add_module` has exactly three callers
  (`examples/px4/cpp/firmware`, `examples/px4/cpp/bridge`,
  `integrations/px4/module-template`), and the only other consumer of a built
  nano-ros artifact in this area, `packages/testing/nros-px4-register-check`,
  calls `px4_add_module` directly and links no nano-ros archive at all.
* **The surviving shim: see above** — it does not reliably survive, so there is
  nothing to chase.

## The class this belongs to also existed one layer up — see #1050

The same "a guard whose predicate cannot observe the case its own message
describes" shape sat in `nros_px4_add_module`'s archive precheck, which ran only
`if(_networked_backends)` — i.e. never for the uORB-only module the bug happens
to. Fixed with this issue's sweep; recorded in #1050.
