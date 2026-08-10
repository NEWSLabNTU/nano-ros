---
id: 493
title: "Two cargo workspace ROOTS share one corrosion target dir, so a mixed
  workspace's umbrella staticlib bundles the nros stack twice and the link dies
  on duplicate `#[no_mangle]` symbols"
status: open
type: bug
area: build
related: [phase-340, phase-344, issue-0492, rfc-0070]
---

## Symptom

`just build-test-fixtures lane=native` reaches the workspace fixtures and dies
on `examples/workspaces/mixed` (`ws-group-10`):

```
ld.lld: error: duplicate symbol: nros_rmw_cffi_register
>>> defined at lib.rs:731 (packages/rmw/cffi/src/lib.rs:731)
>>>   nros_rmw_cffi-df6cce59090cd17a.….rcgu.o in archive libnros_ws_runtime.a
>>> defined at lib.rs:731 (src/lib.rs:731)
>>>   nros_rmw_cffi-a49bb61a295363bc.….rcgu.o in archive libnros_ws_runtime.a
```

…and the same for `nros_rmw_cffi_lookup`, `_register_named`,
`_registered_names`, `_set_custom_transport`.

## What is actually duplicated

**One archive, two whole compilations of the nros stack.**
`libnros_ws_runtime.a` (44 MB) contains two `-C metadata` identities of each of
**ten** crates:

```
atomic_waker  log  nros_core  nros_log  nros_node  nros_params
nros_platform_api  nros_platform_cffi  nros_rmw  nros_rmw_cffi
```

For `nros_rmw_cffi`: 9 objects under `df6cce59090cd17a`, 9 under
`a49bb61a295363bc`.

Both rlibs sit in the same `deps/` and were built **41 s apart in one run**
(13:20:57 and 13:21:38) — so this is not stale accumulation, and wiping the tree
does not fix it.

## Cause

The two debuginfo paths name the mechanism:

| identity | debuginfo path | means |
| --- | --- | --- |
| `df6cce5…` | `packages/rmw/cffi/src/lib.rs` | compiled with the **repo root** as workspace root |
| `a49bb61…` | `src/lib.rs` | compiled as a path dep of a **different** workspace |

Two cargo invocations, two workspace ROOTS, **one `--target-dir`**:

1. `packages/api/nros-cpp/CMakeLists.txt:54` calls `corrosion_import_crate()`
   **unconditionally**, so every consumer that `add_subdirectory`s nano_ros
   builds `--manifest-path <repo>/packages/api/nros-cpp/Cargo.toml` — root
   workspace context.
2. A workspace that has Rust nodes ALSO gets the synthesised umbrella
   (`cmake/NanoRosRuntimeCrate.cmake`, phase-241 W11 "Option D"):
   `nros_ws_runtime`, its own workspace, carrying
   `nros-cpp = { path = "<repo>/packages/api/nros-cpp" }` as an out-of-workspace
   path dep.

Corrosion derives its target dir from `CMAKE_BINARY_DIR` and offers **no
override** — phase-344 measured exactly this — so both land in
`<build>/cargo/build`. Cargo computes a different `-C metadata` per workspace
root, `deps/` accumulates both, and the umbrella staticlib bundles a mix: some
member crates were compiled against one `nros_rmw_cffi`, some against the other.
Every `#[no_mangle]` export then appears twice.

`cargo metadata` on the umbrella reports **one** `nros-rmw-cffi` package, which
is why this does not look like a dependency-graph problem. It is not one — the
graph is fine; the *identity* is not.

Only **mixed** workspaces fail: the collision needs the umbrella (Rust nodes
present) AND the unconditional plain import. Pure-C/C++ and pure-Rust
configures have one root each.

## Why the plain archive is built at all here

In this workspace the plain `libnros_cpp.a` is on **no executable link line** —
it is produced only so a header rule can consume it
(`nros_cpp_config_generated.h` / `nros_config_generated.h`). The umbrella is
what the 25 real link lines use. `NanoRosRuntimeCrate.cmake:19` states the
intended split: "pure-C / pure-C++ workspaces keep `nros-cpp-headers` pointed at
the plain `nros_cpp-static`" — i.e. the plain import was meant as the *fallback*
for configures with no Rust node, but it is imported unconditionally.

## The framing that settles it: ONE implementation provider

In C there is exactly one site that produces the implementation — an archive,
built once — and every other site is *headers plus a link to that archive*. The
symbol implementation has a single source of truth.

Measured against that, the current wiring is already 90 % right and the last
10 % is the bug:

* `nros-cpp-headers` is an INTERFACE library (aliased `NanoRos::NanoRosCpp`)
  carrying only includes, and `target_link_libraries(nros-cpp-headers INTERFACE
  nros_cpp-static)` names ONE implementation provider. Consumers already do
  "scan the header, link the one archive".
* `nros_synth_runtime_umbrella` SWAPS that provider — it rewrites the INTERFACE
  link from `nros_cpp-static` to `nros_ws_runtime-static`.

The swap is not atomic, in two ways, and its own comment admits the first:

> All other INTERFACE wiring (includes, cyclone, stdc++) is preserved;
> **`nros_cpp-static` stays built but unreferenced.**

1. **The displaced provider keeps building.** It is forced by the header mirror,
   which depends on `$<TARGET_FILE:nros_cpp-static>` — deliberately, because
   issue 0268 showed an order-only dep lets the mirror go stale. Unreferenced is
   not free: it shares the corrosion target dir with the referenced provider,
   which is the duplication above.
2. **Headers and symbols come from DIFFERENT providers.** After the swap the
   interface links the umbrella but still mirrors headers produced by the plain
   crate's `build.rs`. Those headers carry the variant storage sizes
   (`*_OPAQUE_U64S`). If the two builds' feature sets ever diverge, the sizes
   header describes code that was not the code linked — the 0088 → 0114 → 0122 →
   0123 → 0245 → 0268 sizes-mirror class, re-entered through a new door.

So "two providers" is the defect, and separate target dirs (option A below)
would preserve it: two archives would still each contain a full copy of the nros
implementation, still be compiled twice, and any binary that ever linked both
would be back here. **A treats the collision, not the duplication.**

### What the fix has to satisfy

*Exactly one provider per configure, owning BOTH the archive and the generated
headers, chosen atomically.*

That is also a hard constraint rather than a preference: a Rust `staticlib` is a
self-contained bundle, so two of them can never appear in one link — which is
why Option D (phase-241 W11) bundles `nros-cpp` into the umbrella in the first
place. The umbrella IS the single-provider design; it is just not yet exclusive.

### Cargo already models this and the tree does not use it

`links = "nros_cpp"` on the providing package declares "this crate provides the
nros_cpp native implementation", and **Cargo enforces that at most one package
in a graph may claim a given `links` name** — the C model's guarantee,
built in. The provider's build script then publishes its artifacts
(`cargo:include=…`, `cargo:root=…`) and every dependent reads them as
`DEP_NROS_CPP_INCLUDE`. Neither `nros-cpp` nor `nros-c` declares `links`, and no
build script emits that metadata.

Two honest limits on what that buys:

* It would **not** have caught this bug. The check is per-graph, and here there
  are two separate cargo invocations each with a perfectly valid single-provider
  graph. Claiming otherwise would be overselling it.
* What it DOES buy is the missing mechanism for fix B: the umbrella's build
  script could read `DEP_NROS_CPP_INCLUDE` and re-export the provider's headers
  to a stable path, so CMake can mirror headers from whichever provider is
  active. Without it there is no supported way to reach the nros-cpp `OUT_DIR`
  inside the umbrella's build.

The invariant that WOULD have caught it is a CMake-level one: **a configure must
build exactly one Rust staticlib exporting the nros symbol set.** That is
checkable and belongs in a gate, in the spirit of `check-fixture-groups`.

## Fixes considered

**A — give the plain import its own target dir.** Isolates the two roots so the
`deps/` collision cannot happen. **Not the fix.** Under the single-provider rule
it is the wrong shape: two archives would still each carry a full copy of the
nros implementation, still be compiled twice, still feed headers from one while
symbols come from the other. It removes the link error and keeps the defect.
Worth stating because it is the obvious first move and it is a trap.

**B — the umbrella becomes the SOLE provider (recommended).** When
`nros_synth_runtime_umbrella` runs, make the swap total rather than partial:

  1. skip the plain `corrosion_import_crate` for that configure — the code
     comment already says the plain archive is the no-Rust-node fallback, so the
     bug is a missing condition, not a missing design;
  2. mirror the generated headers from the UMBRELLA's build, so headers and
     symbols come from one compilation.

Step 2 is the part with no mechanism today, and it is what makes `links` worth
adding: with `links = "nros_cpp"` + `cargo:include` on the provider, the
synthesised umbrella's build script can read `DEP_NROS_CPP_INCLUDE` and
re-export the headers to a stable path for CMake to mirror. Without that there
is no supported way to reach nros-cpp's `OUT_DIR` from inside the umbrella
build.

**C — stop bundling `nros-cpp` in the umbrella and link the plain archive.**
Rejected. Two Rust staticlibs cannot coexist in one link (each is a
self-contained bundle), which is exactly why Option D bundles it; this
reintroduces the `--allow-multiple-definition` problem that design removed.

**Enforcement, either way.** The invariant is *one Rust staticlib exporting the
nros symbol set per configure*. `links` does not enforce it — that check is
per-graph, and here two independent invocations each have a valid single-provider
graph. A CMake-level gate does: assert a configure builds exactly one such
archive, in the spirit of `check-fixture-groups`. Without it, the next consumer
that imports a root-workspace crate re-creates this silently.

## Connection to the phase-340 identity budget

This is very likely the same mechanism behind the disputed identity reading in
this exact tree. `check-artifact-identity-budget` reads
`examples/workspaces/mixed/build-workspace-fixtures`, and on a long-lived tree
here `nros` measures **12** identities = 2 workspace roots × 2 R3 halves × 3
feature sets, against a ceiling of 6 set by another session on a tree where the
duplication had not occurred. Fixing this should collapse the count — and settle
that disagreement — rather than the ceiling needing to move.

## Not verified

Whether this reproduces in the distrobox lane and in CI. It reproduces here on
every `lane=native` attempt, and it is a property of the CMake graph rather than
of the host toolchain, so a host-specific cause is unlikely — but that is
reasoning, not a measurement.
