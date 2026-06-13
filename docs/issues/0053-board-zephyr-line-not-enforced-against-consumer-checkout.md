---
id: 54
title: nros setup board declares a Zephyr line but never verifies the consumer's checkout matches → silent net-API drift
status: open
type: bug
area: zephyr
related: [phase-215, rfc-0014]
---

> Note: filename id 0053 reuses an available slot; the canonical id is **54**
> (the id field is authoritative).

## Symptom

A downstream Zephyr consumer (Autoware Safety Island) building board
`fvp-aemv8r-smp` failed deep in the link/app stage compiling
`nros-platform-zephyr`:

```
net.c:399: error: storage size of 'mreq' isn't known          (struct ip_mreq)
net_wait.c:96: error: 'struct net_if_addr' has no member named 'ipv4'
```

`net_wait.c` deliberately uses the **Zephyr 3.7+** IPv4 layout
(`iface->config.ip.ipv4->unicast[i].ipv4.is_used`, where each unicast entry is
wrapped in `struct net_if_addr_ipv4`). The consumer's `west.yml` pinned zephyr
at `339cd5a45` = **3.5.99** ("zephyr-v3.5.0-507"), whose `net_if_addr` is not
`.ipv4`-wrapped and whose `<arpa>`/socket `ip_mreq` differs — so the 3.7-shaped
platform code does not compile.

## Root cause

The board crate declares `NROS_BOARD_ZEPHYR_LINE = "3.7"`
(`packages/boards/nros-board-fvp-aemv8r-smp/board.cmake`), and
`nros setup board` reads it (`board_metadata.rs` `zephyr_line`) **only** to pick
the patch set `scripts/zephyr/patches/<line>.sh`. It never checks the version of
the consumer's actual `ZEPHYR_BASE` checkout against the declared line. A
consumer whose `west.yml` pins a different zephyr (here 3.5.99) gets the 3.7
patch set applied to a 3.5 tree, Rust provisioned, and **no warning** — the
mismatch only surfaces as net-API compile drift in `nros-platform-zephyr`.

So nano-ros's platform-zephyr code correctly targets the board's declared 3.7
line; the gap is that the board contract is not *enforced*: provisioning is
silent about a consumer running off-line.

## Fix directions

- **`nros setup board` should verify `ZEPHYR_BASE` matches the board's
  `zephyr_line`.** Read the checkout's `VERSION`/`west list -f {revision}
  zephyr` and hard-error (or loud-warn) when the major.minor doesn't match the
  declared line — turning a deep compile drift into a clear provisioning error.
- **Optionally surface the expected zephyr revision/range from the board crate**
  so a consumer's `west.yml` can be validated (or generated) against it, rather
  than each consumer hand-pinning a zephyr that may not match.

## Consumer-side workaround (ASI)

Bump `actuation_module/west.yml`'s `zephyr` revision to a 3.7 release matching
the board's declared line (and the `zephyr-lang-rust` pin in lockstep).
