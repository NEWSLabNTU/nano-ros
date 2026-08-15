---
id: 617
title: "Three embedded link failures from one cause: making default features opt-in removed the providers a `no_std` final artifact needs"
status: open
type: bug
severity: high
area: build, api
related: [issue-0591, issue-0594, issue-0586, issue-0615, phase-361]
---

## Summary

phase-361 is making features opt-in per dep-site (`default = []`, `std` spelled
where needed). The direction is right. Its blast radius on EMBEDDED targets is
larger than the campaign has landed fixes for, and the failures do not name the
cause: each surfaces as a missing language item or a duplicate one, several
crates away from the manifest that changed.

Three instances, all found in one afternoon while trying to get a tier-2 verdict.
They are one bug class, not three bugs.

| # | target | error | status |
| --- | --- | --- | --- |
| 1 | `thumbv7m-none-eabi`, cortex-m C++ leaf | `E0004: non-exhaustive patterns: TransportError::BackendDynamic(_) not covered` | **fixed** (`e5bc6363e`) |
| 2 | `native_sim`, mixed workspace entry | `the #[global_allocator] in nros_platform conflicts with global allocator in: nros_platform` | open |
| 3 | `armv7a-nuttx-eabihf`, NuttX C leaf | `#[panic_handler] function required, but not found` (compiling `nros-c`) | open |

Each takes out a whole fixture family, and #3 and #2 together make
`build-test-fixtures lane=all` unable to finish — so tier 2 and tier 3 cannot
reach a verdict at all.

## Why they are one class

A `no_std` FINAL ARTIFACT needs exactly one provider of each language item — a
`#[panic_handler]`, a `#[global_allocator]` — and needs the cfg that decides
whether a variant EXISTS to agree with the cfg that decides whether the code
handling it exists. Default features were quietly supplying all three:

* **#1** the variant is gated on `nros-rmw/alloc`, the match arm was gated on
  `nros-cpp/alloc`. That implication runs one way, and cargo unifies features
  across the graph, so `nros-cpp`'s `default = []` made "variant present, arm
  compiled out" reachable.
* **#3** `nros-c` still has `default = ["panic-spin"]`, so a dep-site that now
  passes `--no-default-features` without re-adding a panic provider gets a lib
  with no `#[panic_handler]`. The NuttX C leaf is such a site.
* **#2** is the same shape from the other side: with providers being moved
  between crates (`nros-c/global-allocator` forwarding to `nros-platform`,
  phase-361 W8.a/W8.c, issue 0594), one link now sees `nros_platform` twice.

The unifying rule the campaign needs: **when a crate stops defaulting a
provider, every dep-site that builds a final artifact must be audited, not just
the ones whose host build still passes.** A host build cannot detect any of
these — it gets its panic handler and allocator from `std`.

## Why this is filed rather than fixed

#1 is fixed. #2 and #3 are not, deliberately: phase-361 W7/W8 and issue 0615 are
under active edit, with commits minutes old at the time of writing (including
two whose messages record renumbering collisions with parallel sessions). A
third fix landed blind into a moving campaign is how the day's six collisions
happened, twice with a better version already written by someone else.

Whoever owns phase-361 W8 should take #2 and #3 together; they are likelier one
edit than two.

## Reproduce

```sh
just zephyr build-fixtures     # -> #2, mixed workspace entry
just nuttx build-fixtures      # -> #3, nros-c for armv7a-nuttx-eabihf
```

Both were green earlier the same day, on the same host, before the pull that
brought the current phase-361 state.

## Note for whoever fixes #3

`nros-c`'s manifest comments already state the intended contract — "ports that
own the unified kernel heap enable `global-allocator` + `panic-spin`" — so the
NuttX C dep-site is likely just missing that spelling rather than needing a
design change.

## Blocked on this

Investigation of the `rtos_e2e` NuttX C **action** failure (`ensure_ready`: the
server never prints `Waiting for action goals`, while pubsub and service pass on
the same platform and language). The fixture cannot be rebuilt while #3 stands,
so the test can only report STALE. The marker itself is confirmed correct —
`examples/qemu-arm-nuttx/c/action-server/src/main.c:241` prints exactly that
string — so the server genuinely never reaches it, and the init sequence
(`Support initialized` -> `Node created` -> `Action server created: /fibonacci`
-> `Waiting for action goals`) gives a clean bisect the moment the fixture
builds again.
