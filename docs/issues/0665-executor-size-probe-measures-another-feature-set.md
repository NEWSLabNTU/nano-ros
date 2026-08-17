---
id: 665
title: "`EXECUTOR_OPAQUE_U64S` is a size measured under ANOTHER unit's feature set, so the `env` split makes it 16 bytes too small"
status: open
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
