---
id: 2492
title: "A hosted zenoh image's pool-exhaustion diagnostics can be lost: the Rust error never reaches the console, and the C line is block-buffered"
status: resolved
type: bug
severity: medium
area: [rmw-zenoh, diagnostics, testing]
found: 2026-11-13
resolved: 2026-11-13
related: [2482, phase-556]
---

## What

Measured on 2026-11-13 while root-causing issue 2482, on the native Linux
zenoh image of fixture `param-two-node-talker`, whose liveliness pool (16)
is one token short. When the 17th token is refused, two diagnostics are
meant to say so:

- the C shim prints
  `zpico: liveliness pool exhausted (NROS_ZENOH_MAX_LIVELINESS=16) for '<keyexpr>'`
  (`packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c`, around line 3820);
- `declare_keyexpr` in `packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs`
  (around line 1012) logs the refusal with `nros_log::log_error!`, naming the
  knob to raise, then returns `None`; the entity is still created (the missing
  graph token is announced, not fatal, by design).

In the measured run, with the image's stdout and stderr redirected to a file
and `RUST_LOG=debug`:

- the Rust `log_error!` line did not appear in the output at all;
- the C line appeared only when the image ran under `stdbuf -o0 -e0`.
  Without it, `printf` output to a redirected file is block-buffered and is
  lost when the process is killed, which is how every test harness stops an
  image.

So the one image whose only symptom is "a service is missing from
`ros2 node info`" can run, be killed and leave no trace of why. Issue 2482
went unexplained for exactly this reason: `params_per_node_interop` and the
new parity test both capture the image's output and saw nothing.

## Not established

Why the Rust error does not reach the console on a hosted image (logger not
installed, level filter, or routed somewhere else) was not investigated.

## Fix direction

- Make the hosted console unbuffered or line-buffered for diagnostics (or
  flush after each `printk` on hosted), so a killed image keeps its last
  lines.
- Make the Rust `log_error!` reach the console on a hosted image by default.
- Consider a check that fails a fixture run whose output contains a pool
  exhaustion line, so a silent exhaustion becomes a red test.

## Acceptance

A hosted image killed after a refused liveliness token leaves both
diagnostics in its captured output, without `stdbuf`.

## Resolution

Fixed by #3696 (`3cb1b781b1`). The hosted C shim's `printk` flushes each
line, so a signal no longer loses it, and a plain-API `main` gets its log
sinks from `nros_board_linux::register_linked_rmw`, so `nros-rmw-zenoh`'s
record leaves `nros_log`'s early ring. Checked by
`hosted_pool_exhaustion_console::an_exhausted_liveliness_pool_is_on_the_console_before_the_image_is_killed`,
which reads both lines from the live pipe of the
`param-two-node-talker-short-liveliness` image before killing it (passed in
a tier-2 run on 2026-11-14).
