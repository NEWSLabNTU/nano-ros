---
id: 1067
title: "The board device-bringup contract is a doc comment: `TransportBringup` has 0 impls and 0 callers, and the one `NetworkWait` impl is deliberately routed around because it does not link"
status: open
type: bug
area: platform, build
severity: medium
found: 2026-09-05
related: [1063, 1064, phase-206, phase-212, phase-349]
---

# A boot order that nothing executes

## Measured 2026-09-05

| symbol | impls | production callers |
| --- | ---: | ---: |
| `BoardEntry::run` / `run_with_deploy` | **12** of 17 board crates | live — this is the real path |
| `NetworkWait::wait_link_up` | **1** (`nros-board-zephyr`) | **0** |
| `TransportBringup::init_transport` | **0** | **0** |

`packages/platform/nros-platform/src/board/entry.rs:15-20` states the contract:

    init_hardware -> init_transport -> wait_link_up -> open executor -> setup -> spin

**That is a doc comment, not code.** `init_transport` appears nowhere in the tree
outside its own trait definition — no implementation, no call site.

## The `NetworkWait` half is worse, because the bypass is documented

`nros-board-zephyr` implements `wait_link_up`. The one place that would call it
— the `nros::main!` Zephyr arm — deliberately does not, and says why
(`packages/core/nros-macros/src/main_macro.rs:1935-1941`):

> Use the `nros_platform::zephyr::wait_network` C-symbol wrapper … it exposes a
> real linkable symbol. (`ZephyrBoard::wait_link_up` calls Zephyr's
> `net_if_is_up` / `k_msleep`, which are `static inline` header functions with
> no link symbol, so the native_sim final link fails with undefined references.)

So the trait method cannot be called from the emitted entry **at all**, on the
one platform that implements it. The runtime gate is real; it just is not this
trait.

## Where the bring-up actually happens

Inside each board's own `BoardEntry::run`, or the family helper it delegates to
(`nros_board_freertos::run_entry`, which takes the board `Config` carrying
MAC/IP/netmask/gateway and the task priorities). That is a working arrangement.
The defect is not "devices are not brought up" — they are. It is that the
*declared* contract and the *real* one are different things, and only the
declared one is discoverable.

## Why that matters, concretely

The trait is the discoverable surface. A new board author reads
`board/transport.rs`, implements `TransportBringup`, and **nothing ever calls
it** — the board silently never brings its link up, and the failure appears
much later as an RMW session that cannot open. The trait's own doc comment
promises the opposite:

> Phase 212.N.1 — trait surface only. 212.N.2 family driver crates provide
> concrete impls.

212.N.2 *did* land (`d4b2b248c`), and the impls are gone now. So the doc
describes a state that existed and was undone, which is the phase-206 shape
exactly: a foundation removed by later work that did not know it was load-bearing.

## Options

**A. Wire the traits into `BoardEntry::run`.** Make the documented order real:
the default `run` calls `init_transport` and `wait_link_up` when the board
implements them. Cost: the Zephyr linkage problem above is real and would have
to be solved (or `ZephyrBoard::wait_link_up` re-expressed over linkable
symbols), and 12 boards' `run` bodies need auditing for double bring-up.

**B. Delete both mixins** and document `BoardEntry::run` as the whole contract,
since that is what boards actually implement. Cost: `nros-board-zephyr`'s
`wait_link_up` is public API and is used in its README example; deleting it is
a breaking change for an out-of-tree consumer.

**C. Keep them, and make the gap loud** — a gate asserting that a board
implementing `TransportBringup` is reachable from its `run`. Smallest, and it
turns the trap into an error, but it leaves two ways to express one thing.

**Recommendation: B**, with `wait_link_up` kept on `ZephyrBoard` as an inherent
method rather than a trait impl. The declared contract should be the one that
runs.

## The class

Third live instance of declared-but-unread, after `set_interfaces` (deleted,
phase-206 W5) and `NROS_NETSTACK` (issue 1063) — and the first where the
declaration is a TRAIT, so implementing it looks like joining a contract rather
than writing dead code.

## Not covered

* Whether any of the 12 `BoardEntry::run` bodies would double-bring-up if the
  traits were wired in (option A). Unmeasured.
* Whether an out-of-tree board implements either trait. Both are public API.
* `nros-board-nuttx-qemu/src/entry_212n.rs` is named for this phase family and
  may be a partial N.2 survivor; not read.
