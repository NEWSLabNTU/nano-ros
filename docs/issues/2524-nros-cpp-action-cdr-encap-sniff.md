---
id: 2524
title: "nros-cpp's action trampolines guess whether a payload carries a CDR header by sniffing its first two bytes"
status: open
type: tech-debt
severity: low
area: [cpp-api, rmw]
related: [phase-555]
found: 2026-11-14
---

**Home: `packages/api/nros-cpp/src/action.rs`, `payload_has_cdr_encap` and
`frame_for_callback`**

## What it does

Before handing an action payload to a C++ callback, `frame_for_callback`
checks whether the payload already starts with a CDR encapsulation header
(`00 {00|01|06|07|0a|0b}`). If so it forwards it verbatim; otherwise it
prepends a little-endian header.

That is a guess about framing, made from payload bytes. RFC-0069 recorded the
same kind of check in the executor as a defect: a raw payload whose first two
bytes happen to look like a header would be forwarded unframed. The executor
now always splices exactly one header (`arena.rs`, RFC-0069), and its own
sniff survives only under `#[cfg(test)]`. This one is in non-test code.

## Why it is probably unreachable

The comment above the function says the upstream dispatch now delivers
action payloads WITH a single header, splicing one in when a C server's reply
arrives raw. If that holds on every road, the `else` branch never runs, and
the sniff can only misfire on a payload it should have left alone. That has
not been measured.

## Found by

phase-555's design review (rmw-wire brief, "Untracked work found"). The user
ruled that the untracked items be filed as issues (2026-11-14).

## What closes it

Confirm that every road reaching these trampolines delivers a framed payload,
then forward unconditionally and delete `payload_has_cdr_encap`, as the
executor did. If some road delivers raw, frame it there, where the framing is
known, rather than guessing here.
