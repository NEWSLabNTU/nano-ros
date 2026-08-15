---
id: 616
title: "`ws-mixed-entry-zenoh` fails to build: two `nros_platform` units, so the
  one `#[global_allocator]` in the tree collides with itself"
status: resolved
type: bug
area: build, platform
related: [issue-0594, issue-0500, issue-0614, phase-361]
---

## Symptom

```
$ just zephyr build-fixtures
…
zephyr-fixture-70-build-ws-mixed-entry-zenoh failed
error: the `#[global_allocator]` in nros_platform conflicts with global allocator in: nros_platform
error: could not compile `nros-cpp` (lib) due to 1 previous error
```

One crate named on both sides. Reproduces on its own:

```
cmake --build ~/repos/nano-ros-workspace/build-ws-mixed-entry-zenoh
```

It is the ONLY fixture failing this way — the other 69, including every other
workspace entry and all three cortex-m leaves, build.

## Why it is possible at all

`nros-platform` declares the allocator once:

```rust
// phase-361 W8.c / issue 0594 — this is the ONE `#[global_allocator]` in the
// tree. `nros-c` used to define a second one under an identical gate …
#[cfg(all(feature = "global-allocator", not(feature = "std")))]
mod global_allocator {
```

Issue 0594 removed the second declaration precisely so a duplicate lang item
would be *impossible rather than merely discouraged*, on the reasoning that
"cargo unifies one crate's one feature into one unit". That reasoning holds for
ONE unit. This failure is what it looks like when there are two: the same crate,
the same feature, resolved twice, each contributing the lang item.

So the interesting question is not the allocator — it is why `nros_platform`
appears twice in this graph and not in the other 69.

## What the dependency graph says — and it contradicts the error

`build-ws-mixed-entry-zenoh/nros_ws_runtime/Cargo.toml` asks for:

```toml
nros-cpp = { path = "…/packages/api/nros-cpp", default-features = false,
             features = ["rmw-zenoh-cffi", "platform-zephyr", "ros-humble", "std"] }
```

built as

```
cargo build -p nros_ws_runtime --no-default-features
    --profile nros-relwithdebinfo --target x86_64-unknown-linux-gnu
```

Asking cargo directly:

```
cargo tree --manifest-path <generated> -p nros_ws_runtime -i nros-platform \
    -e features --target x86_64-unknown-linux-gnu
```

reports **exactly one** `nros-platform v0.5.0`, with `global-allocator` enabled
and **no `std`** on `nros-platform` itself — so the
`cfg(all(global-allocator, not(std)))` module is legitimately compiled, once.

That is the whole puzzle: cargo resolves ONE unit, rustc rejects the build for
having TWO, and names `nros_platform` on both sides. One of the two is not in
the graph cargo printed. Candidates, in the order worth testing:

* **A second Rust artifact linked beside the rlib.** The same cmake target also
  builds `nros-rmw-zenoh-staticlib` (step `[4/7]` in the failing log). A
  staticlib carries its own compiled copy of its dependencies, `nros-platform`
  among them, and that copy is invisible to `cargo tree` for this manifest.
* **Two `-C metadata` identities for one crate**, e.g. host-graph vs target-graph
  builds of `nros-platform` both reaching the final rustc invocation. Note the
  target here IS the host triple (`native_sim`), which is exactly when those two
  graphs can collide instead of staying separate.

Whichever it is, the fix should make it structural: issue 0594's stated
guarantee — "cargo unifies one crate's one feature into one unit" — is true of a
graph and false of a link, and this is the case that shows the difference.

## Not

* **Not caused by the `cpp_diag!` / issue 0589 work**, which was in flight when
  this surfaced. Verified by rebuilding the same fixture with
  `packages/api/nros-cpp/src/lib.rs` stashed: identical failure, same message.
* **Not issue 0614** (`cargo check -p nros-c` with no features). Same
  feature-contract neighbourhood, different failure: 0614 is a missing panic
  handler on a bare host check, this is a duplicate lang item in a fully
  specified build.
* Not a fixture-staleness artifact: it reproduces from a clean
  `cmake --build` of that directory.

## Investigation 2026-08-16 — two eliminations, then the mechanism

**Not the generated manifest's feature set.** Reconstructed the failing runtime
manifest exactly — `nros-cpp` with
`["ros-humble", "rmw-zenoh-cffi", "std", "platform-zephyr"]`, plus
`rust_heartbeat_pkg`, `crate-type = ["staticlib"]`, host triple — and it BUILDS
CLEAN:

```
cargo build --manifest-path <reconstructed> -p nros_ws_runtime \
    --target x86_64-unknown-linux-gnu
    Finished
```

The graph finding above is confirmed independently, for the posix variant (the
in-tree `build-workspace-fixtures/nros_ws_runtime`) and this zephyr
reconstruction: `cargo tree -d` reports **no duplicate `nros-platform`**. It does
report `nros-core`/`nros-rmw`/`nros-serdes` twice — that is the host-graph vs
target-graph split issue 0591 records as legitimate, not this.

**Not a shared target dir.** Built `nros-rmw-zenoh-staticlib` (the `[4/7]` step)
and then the runtime into one `CARGO_TARGET_DIR`. Both succeeded.

**The mechanism is the LINK, and it is visible in the archives.** Every
`crate-type = ["staticlib"]` root bakes the allocator into its own `.a` when
`global-allocator` is on. Measured with `nm`:

```
libnros_c.a                     ___rustc___rust_alloc, ___rustc___rust_alloc_zeroed,
                                ___rustc___rust_alloc_error_handler, and
                                nros_platform::global_allocator::PlatformGlobalAllocator's
                                GlobalAlloc impl — all global `T`
libnros_rmw_zenoh_staticlib.a   the same allocator symbols
```

There are **four** such roots in `packages/`: `nros-c`, `nros-cpp`,
`nros-rmw-zenoh-staticlib`, `nros-rmw-xrce-cffi-staticlib`. And both sides of
this fixture request the allocator, by different routes:

```
nros-cpp/platform-zephyr -> nros-c/platform-zephyr -> "global-allocator"
                                                   -> nros-platform/platform-zephyr

nros-rmw-zenoh-staticlib/platform-zephyr-baremetal -> nros-platform/global-allocator
                                                   -> dep:panic-halt
```

So an image linking the ws-runtime staticlib AND the backend staticlib links two
allocators. Nothing about that is visible to `cargo tree`, which is why the graph
and the error disagree.

## Why the current design cannot hold this invariant

`#[global_allocator]` is a lang item: **unique per LINKED ARTIFACT**. nano-ros
declares it in `nros-platform`, a mid-graph library, gated on a feature — and
issue 0594's guarantee, "cargo unifies one crate's one feature into one unit",
is a property of ONE graph. A staticlib is not a graph; it is a sealed copy of
one. Four sealed copies can each contain the item and each be individually
correct.

`check-feature-contract` clause (e) has the same blind spot by construction: it
counts `#[global_allocator]` DEFINITIONS IN SOURCE, and there is exactly one.
The count it should be making is per produced archive.

## Fix options

1. **One link root per image, enforced.** phase-241 W11 already designed
   `nros-cpp` as "the ONE Rust staticlib a C++ binary links". Make that checked
   rather than conventional: the backend staticlibs stop being independent link
   roots (become rlibs bundled into the root), or stop requesting
   `global-allocator`/`panic-*` — those lang items belong to whoever owns the
   image, and a backend does not.
2. **Move the item to the root crate.** `nros-platform` keeps providing the
   `GlobalAlloc` TYPE; the `#[global_allocator]` STATIC is installed by the link
   root through a macro (`nros_platform::install_global_allocator!()`). "One per
   image" then means "one root", which the build system already controls, rather
   than "one unit", which it does not.
3. **A link-side gate, complementing either.** `nm` the produced archives and
   assert at most one defines `___rust_alloc` per image. This is the check
   clause (e) cannot make from source, and it is the layer the invariant
   actually lives at.

(1) is the smaller change and matches the existing intent; (2) is the one that
makes the error impossible rather than caught. They compose: (2) plus (3) would
leave no way to express the bug.

## Still unreproduced here

This host has no Zephyr workspace, so `cmake --build build-ws-mixed-entry-zenoh`
could not be run. The mechanism above is established from the archives and the
feature graph, NOT from the failing build. What would confirm it directly: the
`--extern` lines, or `nm` on the two archives the failing link consumes.

## Acceptance

* `ws-mixed-entry-zenoh` builds;
* the reason there were two `nros_platform` units is written down — if the
  answer is "a feature set disagreed across dep paths", the fix should make that
  disagreement impossible or loud, since 0594's stated guarantee ("cargo unifies
  one crate's one feature into one unit") is what this violated;
* something covers the mixed entry, which is the only fixture that caught this.


## 2026-08-16 — root cause: two cargo workspace roots sharing one `--target-dir`

Fixed. The cause is neither of the two candidates guessed above, and the
dependency graph was never wrong — `cargo tree` reporting one `nros-platform`
was correct, because it can only report on ONE workspace at a time and this
build uses two.

`ws-mixed-entry-zenoh` runs four cargo invocations into one `--target-dir`
(`<build>/nros-rust`), and they do not all belong to the same workspace:

| invocation | `--manifest-path` | workspace |
| --- | --- | --- |
| `-p nros-c` | `nano-ros/Cargo.toml` | nros root |
| `-p nros-cpp` | `nano-ros/Cargo.toml` | nros root |
| `-p nros-rmw-zenoh-staticlib` | `nano-ros/Cargo.toml` | nros root |
| `-p nros_ws_runtime` | `<build>/nros_ws_runtime/Cargo.toml` | **generated** |

Cargo's `-C metadata` for a crate includes the path spelling it was reached
by. Inside the nros workspace `nros-platform` is a MEMBER; from the generated
`nros_ws_runtime` workspace it is an external path dependency. Same package,
same features, two spellings — so two units, in one `deps/`.

Both carry the allocator, because `nros-platform` holds the tree's one
`#[global_allocator]` (issue 0594):

| unit | features | `__rust_alloc` defined | recorded path |
| --- | --- | --- | --- |
| `1d11b987` | `["global-allocator", "platform-zephyr"]` | yes | `/home/aeon/repos/nano-ros/…/lib.rs` |
| `6dafa462` | `["global-allocator", "platform-zephyr"]` | yes | `packages/platform/…/lib.rs` |
| `5d895365` | `[]` | no | — |

The two allocator units' fingerprints differ in exactly ONE field: `path`.
Features, deps, profile, rustc, target and rustflags are byte-identical.

A compile fails when it resolves a transitive `nros_platform` by searching
`-L dependency=` (transitive deps get no `--extern`) and binds the second copy.
That is why it was intermittent — the two units always existed, and only which
one got bound varied.

### Reproduction, from an empty target-dir

```
root-workspace build (-p nros-cpp)        -> 1 libnros_platform-*.rlib
generated-workspace build (-p ws_runtime) -> 2 libnros_platform-*.rlib
```

Deterministic. Both spellings appear in the `.d` files, and every unit carries
an mtime from that one build — not a leftover, which is what an earlier pass of
this investigation wrongly concluded from the 6-vs-3 rlib count. (Six was two
build generations of a genuine three; the three were never the anomaly.)

### Fix

`zephyr/cmake/nros_cargo_build.cmake` now derives `CARGO_TARGET_DIR` from the
cargo WORKSPACE ROOT rather than sharing one. The nros workspace keeps
`<build>/nros-rust` so every existing consumer path is unmoved; a foreign root
gets `<build>/nros-rust-ws-<name>`.

The root is resolved with `cargo locate-project --workspace`, not by comparing
paths: `packages/cli/Cargo.toml` is a separate workspace INSIDE the repo, so a
path-prefix test would have put it straight back into the shared directory.

Nothing was lost by splitting. Units are keyed by that same path spelling, so
two workspace roots can never reuse each other's artifacts — the shared
directory produced collisions and no sharing.

Backed by an assertion rather than a naming convention: a second workspace root
claiming an already-claimed target-dir is now a configure-time `FATAL_ERROR`
naming both claimants.

### Same class as issue 0500

CLAUDE.md already records this shape one lane over: *"Corrosion < 0.6.0 shares
one `cargo/build` across workspace roots ⇒ duplicate `#[no_mangle]` ⇒ `mixed`
cannot link."* Identical mechanism — one cargo artifact directory serving two
workspace roots, surfacing as a duplicate symbol — in the Corrosion lane rather
than the Zephyr one. `mixed` is the entry that caught it both times, because it
is the only one that pulls both a C and a C++ runtime into a generated
workspace alongside the root one.
