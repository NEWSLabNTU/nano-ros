---
id: 801
title: CONFIG_NROS_DOMAIN_ID reaches the node token but not the entities
status: open
---

# 0801 — the domain splits across entities in one session

Built with `CONFIG_NROS_DOMAIN_ID=10` (present in the build's `autoconf.h`), the
zephyr C listener registers its entities across **two** domains in a single
session — same ZID throughout, so this is one runtime, not stale output:

    @ros2_lv/10/d1cd18cb.../0/0/NN/%/%/node                      <- 10
    Allocating sub decl for (0/chatter/std_msgs::msg::dds_::String_/*)   <- 0
    @ros2_lv/0/d1cd18cb.../0/0/NN/%/%/listener                   <- 0
    @ros2_lv/0/.../0/3/MS/%/%/listener/%chatter/...              <- 0

Counted: 1x `@ros2_lv/10`, 2x `@ros2_lv/0`, subscription key on `0/`.

## Why it is nasty

The domain is the FIRST element of every key `rmw_zenoh` matches on, so a split
is invisible at every layer that reports anything: the session opens, entities
register, the router accepts the link, the board publishes, and `ros2 topic list`
simply never matches. No error is produced anywhere.

Measured effect on real hardware (S32K344 over serial): `ros2 topic echo` worked
in 1 run out of 6 with the board provably healthy every time.

## Where it comes from

Two sources disagree:

  - the SESSION is opened from `RmwConfig`, which carries the Kconfig value —
    `cffi/src/lib.rs`: `CffiSession::open(config.locator, mode, config.domain_id, ...)`,
    and `shim/session.rs` declares the node token with that same `config.domain_id`;
  - ENTITIES resolve through `nros-c/src/node.rs::resolve_session_and_domain`,
    which reads `support.domain_id` (from the C ABI argument, where 0 means
    "unset") or falls back to a hardcoded `0`.

A caller that never sets the C-side domain therefore gets a session on the
configured domain and entities on 0.

## Ruled out: `nros-c/src/node.rs::resolve_session_and_domain`

That function does fall back to a hardcoded `0` when no support context is
present, which is wrong on its own terms, and it was fixed to inherit the
session's domain instead (`CffiSession` now records the domain it was opened
with). **It is not this bug.** The listener image is byte-identical after the
change, including after deleting the Rust build directory and rebuilding — so
that code is not linked into a zephyr C example at all.

## Where it must actually be

The entry reaches the RMW through the C++ header path, not through nros-c:

    zephyr_entry_main.cpp  ->  ::nros::create_node(node, "listener")
                           ->  Node::create  ->  (C++ FFI)

and the domain gets in via `ZephyrBoard::run_components`, which is correct --
`main.hpp:367` forwards `NROS_ENTRY_LOCATOR, "node"` to the 3-arg form, which
passes `static_cast<uint8_t>(NROS_ENTRY_DOMAIN_ID)`, and `NROS_ENTRY_DOMAIN_ID`
resolves to `CONFIG_NROS_DOMAIN_ID` = 10. That matches the observation: the NODE
liveliness token is on 10.

So init carries 10 and the node token proves it, while every entity created
afterwards lands on 0. The remaining suspect is whatever the C++ `Node::create`
path hands to publisher/subscriber creation -- something there is not reading
back the value init resolved.

Worth noting `LinuxBoard::run_components` deliberately calls
`nros::init(nullptr, 0, sn)` so a host build picks `ROS_DOMAIN_ID` out of the
environment. That is correct for a host and would be exactly wrong if any
embedded path shared it, since there is no env on the target -- worth checking
whether the two share code.

## Reproduce

Build any zephyr C example with `-DNROS_ZENOH_DEBUG=3`, read the log, and:

    grep -oE '@ros2_lv/[0-9]+' <trace> | sort | uniq -c

A correct run shows one domain. Discovery-affecting: this should be a gate.

## Narrowed further (2026-08-26), still open

Three candidate sites tried and measured on hardware. All three left the split
exactly as it was -- `1x @ros2_lv/10`, `2x @ros2_lv/0`, subscription key on `0/`:

1. `nros-c/src/node.rs::resolve_session_and_domain` -- hardcoded `0` fallback.
   Fixed to inherit the session's domain. **Not this bug**: that crate is not
   linked into a zephyr C example; the image is byte-identical after the change,
   including after deleting the Rust build directory.
2. `nros-cpp/src/{publisher,subscription}.rs` -- `TopicInfo::with_domain(ctx.domain_id)`.
   Changed to take the domain from the session instead. No effect, so
   `session.domain_id()` is ALSO 0. Reverted rather than left as churn.
3. Same, reading the primary session unconditionally. No effect either.

What that leaves. `CffiSession` is opened as
`CffiSession::open(config.locator, mode, config.domain_id, config.node_name)`
and `shim/session.rs` declares the node token from that same `config.domain_id`
-- and the node token demonstrably comes out on 10. So one field of one config
reaches the node token as 10 and the same field reaches every entity as 0.

The remaining explanations are (a) there are two RmwConfig/session instances and
the entity path resolves the wrong one, or (b) `ctx.domain_id` in `CppContext` is
never written for this entry shape -- `nros_cpp_init` writes it into
`Node::global_storage()`, and it is worth confirming the entity path reads that
same buffer rather than a zero-initialised one.

The next concrete step is a one-line diagnostic rather than another candidate
fix: print `config.domain_id`, `ctx.domain_id` and `session.domain_id()` at
entity creation and see which of the three is 0. Three guesses have now cost
three build/flash cycles each; the measurement is cheaper.
