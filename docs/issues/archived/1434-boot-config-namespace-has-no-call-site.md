---
id: 1434
title: "The baked namespace reaches no runner — `nros_boot_config_namespace()`
  has no call site and every board hardcodes `namespace: None`"
status: resolved
type: bug
area: boot, boards
related: [rfc-0045, 0794, 1410, 1442, 1443, 1444]
resolved_in: 2026-09-21
---

## Problem

The `.nros_boot_config` blob states a launch-declared namespace on BOTH
producers now — the C/C++ emitter since issue 0794, `nros::main!` since issue
1410 — and `BootConfig::from_baked` reads the bit correctly. What is missing is
a CONSUMER, on either road.

**C/C++.** The generated entry reads `nros_boot_config_node_name()` and nothing
else from the blob; its locator and domain reach the runner through the
`NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID` compile definitions, and its
namespace reaches nothing at all:

```c
int main(int, char**) {
    return ::nros::board::LinuxBoard::run_components(
        nros_boot_config_node_name(&NROS_BOOT_CONFIG), &__nros_entry_setup);
}
```

**Rust.** Every board that reads the blob drops the field on the floor —
measured, six sites, all spelled the same way:

```
packages/boards/nros-board-freertos/src/entry.rs:326:        namespace: None,
packages/boards/nros-board-freertos/src/entry.rs:824:        namespace: None,
packages/boards/nros-board-threadx/src/entry.rs:632:        namespace: None,
packages/boards/nros-board-threadx/src/entry.rs:1024:        namespace: None,
packages/boards/nros-board-mps2-an385/src/entry.rs:173:        namespace: None,
packages/boards/nros-board-esp32-qemu/src/board_entry.rs:191:        namespace: None,
```

Each takes `baked.node_name` (and, since issue 1050, `baked.rmw`) off the
resolved `BootConfig` and then writes `namespace: None` into the
`ExecutorConfig` it resolves. `nros-board-linux` does not read the blob at all
beyond the name — `lib.rs:313`, "Only `node_name` is mapped from the overlay;
locator/domain/namespace stay `None` so env keeps authority over them".

So the blob is TRUE — which is what RFC-0045's post-link patch tool and the
`nros_boot_config_*` accessors need — without being the delivery mechanism for
the PRIMARY SESSION's namespace on any platform.

## What is NOT broken, and why this is not urgent

A per-node registration already carries its own namespace: `nros::main!` emits
`runtime.node_identity = Some(("remap_talker", "/island"))` from the same FQN
split (issue 1172), and `NodeRecord` creates the node with it. Measured in the
expansion of `native_rust_remap_entry`. The gap is the SESSION rung — the
identity a board opens the executor with when nothing per-node names one, and
the field a post-link patcher would rewrite.

## Why it keeps getting carved out

Issue 0794 and issue 1410 both recorded this rather than closing it, because it
is a different SHAPE of change from either: it means giving the C-ABI board
runners (`run_components` / `run_tiers`, ten board crates) a namespace
parameter plus the parity ledger that pins that ABI, and deciding per board
whether the baked rung or the board `Config` wins. Neither the emitter fix nor
the macro fix touches a board.

## Acceptance

A C, a C++ and a Rust image whose launch declares a namespace open their
primary session under it — asserted on the ON-WIRE name, since the blob half is
what already works. The negative direction matters as much: an image whose blob
leaves `BOOT_SET_NAMESPACE` clear must keep whatever the next RFC-0045 rung
says, never `""` — and on the hosted board `$NROS_NODE_NAMESPACE` must still
outrank the bake (`namespace_resolves_over_all_three_rungs`).

## Resolution

Resolved 2026-09-21. The namespace now reaches the runner on BOTH roads. What
it does after that turns out to differ by road, and the difference is the part
worth reading.

### Rust — eight sites, one spelling

The grep in the problem statement found six. The class is eight:
`nros-board-nuttx`'s `run_entry` and `run_tiers` build their `ExecutorConfig`
with the BUILDER and pull exactly `node_name` off the bake, which is the same
defect in a spelling `namespace: None` cannot match. So the fix is one function
rather than eight edits — `BootConfig::over_board_defaults(locator, domain_id,
default_node_name)` in nros-node: identity (name, namespace) from the bake,
connect facts from the board, `rmw` from the bake as issue 1050 left it. Folding
NuttX in also gave it the `rmw` rung it had been missing for the same reason, and
turned an out-of-range baked domain into the resolver's loud `DomainIdRange`.

The hosted board is its own rung. `try_resolve_with` already reads
`env.namespace.or(baked.namespace)`, so passing the blob's namespace as the baked
rung costs `$NROS_NODE_NAMESPACE` nothing — `namespace_resolves_over_all_three_rungs`
is unchanged and still passes. `boot_hosted` read only `node_name` out of
`deploy.boot_config`; `run_tiers` read NOTHING (`ExecutorConfig::from_env()` is
`resolve_hosted` over an empty `BootConfig`). Both supply the namespace now;
`run_tiers` still does not supply `node_name`, which is issue 1442.

### C and C++ — the pipe was shorter than this issue assumed

`nros_cpp_init` has ALWAYS taken a namespace. Every caller passed NULL. So the
missing links were the board adapters and the two single-executor C-ABI runners,
not the whole `run_components` / `run_tiers` ABI this issue priced:

* `nros::init(locator, domain, session_name, node_namespace)` — new overload,
  the 3-arg one delegates with `nullptr`, one body and one ladder. It needs its
  own `friend` line in `rclcpp::Node`.
* `<nros/main.hpp>`: a namespace-carrying `run_components` on every board class,
  each old arity delegating down. Header templates, so no ABI moves.
* `nros_board_native_run_components_named_ns` and
  `nros_board_rtos_run_components_ns` — ADDITIVE symbols, the older spellings
  forwarding with NULL. That is `nros_cpp_init_rmw` over `nros_cpp_init`
  (issue 1050) one layer up, and the reason is concrete: an entry TU generated
  before this still calls the older name.

`BoardFamily::c_abi_runners` names the `_ns` spellings, which is what moved 40
goldens — one call line and one banner each.

### The negative direction

NULL and empty both mean "the image declares none" and resolve to the ROOT, at
three edges independently (the C++ header, the Rust runner, the C runner), never
to `""`. On the Rust side `over_board_defaults` passes ABSENCE through as `None`;
`an_unbaked_namespace_stays_absent_through_the_board_rung` asserts that on the
`Option`, because `Some("")` and `None` resolve to the same string and nothing
downstream could tell them apart.

### What was observed ON THE WIRE, and what was not

A hosted Rust image driven through `LinuxBoard::run_with_deploy` with a real
`.nros_boot_config`, against `rmw_zenohd`, three runs:

| bake | env | `ros2 node list --no-daemon` |
| --- | --- | --- |
| `/island` | — | `/island/probe` (+ `/probe`, see below) |
| none | — | `/probe` |
| `/island` | `NROS_NODE_NAMESPACE=/fromenv` | `/fromenv/probe` |

So the positive direction, the negative direction and the precedence rung were
all measured, not argued.

The C road was linked and run and is NOT namespaced on the wire, and that is a
finding rather than a failure of this fix: a C image shaped like the generated
entry reports `/cprobe`, because `nros_cpp_node_create` writes `"/"` for every
node (issue 1443) and `nros_cpp_publisher_create` reads the namespace off the
NODE HANDLE. The session's own liveliness token is at the root on EVERY road,
including with `$NROS_NODE_NAMESPACE` and therefore since long before this
issue, because the CFFI seam drops `RmwConfig::namespace` (issue 1444). So on
the C and C++ roads this change delivers the value correctly into
`ExecutorConfig` — which is what a post-link patcher and every rung above need,
and what this issue asked for — and there is currently no wire-visible consumer
of it there. **C++ was never run**: `just check cpp` compiles and links the
surface, and no C++ image was put on a bus.

### Follow-ups

* 1442 — `run_tiers` carries no namespace on any road (four C-ABI runners), and
  the hosted Rust one carries no `node_name`.
* 1443 — a generated C or C++ entry creates every node at the root.
* 1444 — the CFFI session open drops the namespace, so every image advertises
  its session node at the root.
