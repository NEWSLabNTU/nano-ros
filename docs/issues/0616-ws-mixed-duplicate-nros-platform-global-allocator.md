---
id: 616
title: "`ws-mixed-entry-zenoh` fails to build: two `nros_platform` units, so the
  one `#[global_allocator]` in the tree collides with itself"
status: open
type: bug
area: build, platform
related: [issue-0594, issue-0614, phase-361]
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

## Acceptance

* `ws-mixed-entry-zenoh` builds;
* the reason there were two `nros_platform` units is written down — if the
  answer is "a feature set disagreed across dep paths", the fix should make that
  disagreement impossible or loud, since 0594's stated guarantee ("cargo unifies
  one crate's one feature into one unit") is what this violated;
* something covers the mixed entry, which is the only fixture that caught this.
