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

## Proposed fix (in progress)

Record the domain on `CffiSession` at open and inherit from it wherever the
support context does not supply one, so the two cannot disagree. The session's
value is authoritative: it is what the backend actually received.

## Reproduce

Build any zephyr C example with `-DNROS_ZENOH_DEBUG=3`, read the log, and:

    grep -oE '@ros2_lv/[0-9]+' <trace> | sort | uniq -c

A correct run shows one domain. Discovery-affecting: this should be a gate.
