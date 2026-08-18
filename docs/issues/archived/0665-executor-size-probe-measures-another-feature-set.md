---
id: 665
title: "`EXECUTOR_OPAQUE_U64S` is a size measured under ANOTHER unit's feature set, so the `env` split makes it 16 bytes too small"
status: resolved
type: bug
severity: high
area: build/api
related: [issue-0464, issue-0472, issue-0616, issue-0131, phase-359, phase-340]
---

## Symptom

`just build-test-fixtures lane=native` fails building the `examples/workspaces/c`
CycloneDDS variant:

```
error[E0080]: evaluation panicked: EXECUTOR_OPAQUE_U64S too small for Executor
  + backing — increase NROS_EXECUTOR_ARENA_SIZE or NROS_EXECUTOR_MAX_CBS
  --> packages/api/nros-c/src/executor.rs:56
```

The assert is doing its job: issue 0464 records it catching a committed NuttX
constant that had rotted ~11 % low, and issue 0131 records what the same
mismatch costs when nothing checks — a carved backing running past its buffer
into `.bss`, surfacing as `jalr -> 0` on threadx-riscv64.

## Measured

One shared cargo target dir holds TWO `nros-node` units:

```
nros-node-1b210d8284ad4b6a  ["alloc", "env", "log", "rmw-cffi", "ros-humble", "std"]
nros-node-f42bbb3b5a8203f1  ["alloc",        "log", "rmw-cffi", "ros-humble", "std"]
```

and, in the same tree, two generated `EXECUTOR_OPAQUE_U64S`:

```
11191   nros-c-84923dcf5ed89204
11189   nros-c-3f96050a09813182   (and five siblings)
```

The delta is 2 u64 = **16 bytes**, exactly one fat pointer — the width a
feature-gated slice field adds to `Executor`. The two feature sets differ only
in `env`.

## Cause

`EXECUTOR_OPAQUE_U64S` is not computed from the knobs its error message names.
It is a MEASUREMENT: `packages/api/nros/src/sizes.rs` does

```rust
export_size!(pub EXECUTOR_SIZE = nros_node::ExecutorInlineStorage);
```

…in the `nros` facade's compilation, which `nros-c`'s build script reads through
`DEP_NROS_NODE_*` and divides by 8. The value is therefore `size_of` **under the
feature set the facade was built with**, while the assert compares it against
`size_of` under the feature set THIS `nros-c` links. Before `env` existed those
were always the same, so one number served both.

phase-359 W10 (`1badb6f72`, `03ca659c8`) made `env` an independent capability —
correctly, and for good reasons — and consumers now differ in whether they
enable it. The moment two feature sets coexist, a size measured in one is wrong
for the other, and the shared cargo group dir (phase-340) puts both in the same
tree where either may be probed.

So the error message is also misleading: raising `NROS_EXECUTOR_ARENA_SIZE`
would not fix it. Nothing is undersized — two answers to "how big is this
struct" are being compared, and they are answers to different questions.

## Not caused by the interface-tier change that exposed it

Issue 0663 added a source-tree tier so a ROS-less checkout can resolve
`std_msgs`. That let this workspace's Cyclone variant compile far enough to
reach the assert; it did not create the mismatch. The two-feature-set split is
present in workspace trees untouched by that change (`examples/workspaces/cpp`,
both the default and `-xrce` variants show two distinct `nros-node` feature
sets), and those builds pass — they simply never pair a probe from one set with
a link against the other.

## Directions

1. **Measure where it is used.** Have `nros-c` compute the size from ITS OWN
   dependency rather than reading a facade-built number — same crate graph, same
   features, one derivation. The probe exists because cbindgen cannot recurse
   into deps; that argues for emitting the C macro from the same unit that
   asserts, not for measuring elsewhere.
2. **Or make the number feature-explicit**, so a mismatch is a build error
   naming both feature sets instead of a byte count.
3. Either way the message should stop naming knobs that cannot fix it.

Worth deciding alongside phase-359's remaining `std` work, since that campaign
is what makes feature sets diverge per consumer.

## Resolution 2026-08-18 — direction 3 taken; direction 1 declined; and the first fix only covered one of the two crates

### The forwarding fix was formatting-dependent

`174542aba` fixed the cause — the probe forwarded the caller's `CARGO_FEATURE_*`
intersected with the names the facade declares, missing every
differently-named forward such as `nros-c`'s `std = [… "nros/env" …]`. It added
`features_enabled_on_dep`, which reads the caller's own `[features]` table and
collects each `<probed>/<feat>` an ACTIVE caller feature enables.

That parser took **one line per feature**. `nros-c`'s `std` happens to be a
single line, so the crate that reported the bug was fixed. `nros-cpp`'s is
wrapped:

```toml
std = [
    "alloc",
    # … several paragraphs of prose …
    "nros/env",
    "nros-node/env",
```

Continuation lines have no `=`, so `split_once('=')` skipped them and the scan
returned nothing at all for `nros-cpp`. Neither crate declares an `env` feature
of its own, so `CARGO_FEATURE_ENV` is never set and the manifest scan is the
ONLY route — meaning the sibling was still building its probe without `env`
after the fix, exactly as before it.

It did not fail loudly the way `nros-c` did because `CPP_EXECUTOR_OPAQUE_U64S`
covers `CppContext` (executor + domain + carved backing), whose build.rs
overhead had more than 16 bytes of slack. So the assert passed over a number
that was still short of the true `ExecutorInlineStorage` — issue 0472's hazard
in its quiet form: a C caller's `uint64_t _opaque[]` sized from a
measurement nobody had checked.

Fixed by making the scan read wrapped arrays (tracking an open row, stripping
comments), with two guards:

* `a_wrapped_feature_array_is_read_like_a_single_line_one` — `nros-cpp`'s exact
  shape, prose inside the array included.
* `the_real_c_and_cpp_manifests_forward_env_to_the_probed_facade` — the same
  rule against the **real** `packages/api/nros-{c,cpp}/Cargo.toml`. A synthetic
  fixture cannot notice a manifest being reformatted or gaining a forward;
  reading the files can.

Mutation-checked by restoring the one-line-only behaviour: both fail, and the
real-manifest one is what proves `nros-cpp` was affected on `main`.

### Direction 3 — the message named knobs that cannot fix it

Taken. Both asserts said *"increase `NROS_EXECUTOR_ARENA_SIZE` or
`NROS_EXECUTOR_MAX_CBS`"*. Those move BOTH sides of the comparison equally and
can never close a feature-set gap; following them is a detour of arbitrary
length. The message now says what the number is — a MEASUREMENT taken while
building the facade in the sizes probe, compared against `size_of` in the unit
that links — states that they diverge only when the probe built under a
different feature set, names this issue and the forwarded-set function to check,
and says explicitly that the two knobs cannot close the gap.

### Direction 1 — declined, with the reason

"Have `nros-c` compute the size from its own dependency" cannot be done as
stated. The number's other consumer is a **C macro in a generated header**,
which must be a literal, and the header is written by `build.rs` — before the
crate compiles and with no access to `size_of` of a dependency type. That is why
the probe exists at all; removing it would mean giving the C side no number.

What direction 1 is *right* about is that the assert in the linking unit is the
authority. It already is: it is the thing that caught this. So the work is to
make its failure legible (direction 3, done) and to make the divergence
impossible to reintroduce silently (the two guards above), rather than to move
the measurement.

### Direction 2 — not taken, and why

"Make the number feature-explicit so a mismatch is a build error naming both
feature sets" needs the probe's feature set to reach the const-assert as a
string. A const `assert!` message must be a literal, so this means generating
the assert from `build.rs` with both sets baked in — which moves the assertion
away from the type it is about, for a message the corrected text now conveys
without the move. Worth revisiting only if a third crate joins the two.
