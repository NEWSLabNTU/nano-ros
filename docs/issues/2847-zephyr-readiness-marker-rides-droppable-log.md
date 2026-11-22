---
id: 2847
title: "A Zephyr cell's readiness marker is a deferred log line, so a log burst can drop it"
status: open
type: bug
severity: low
area: [zephyr, testing, logging]
found: 2026-11-23
related: [2825, 2815]
---

## What happens

The mps2/an385 cortex-m cells (`zephyr_cortex_m_qemu`) wait for
`NET_STACK_READY_MARKER`, Zephyr's own `net_config` line `IPv4 address:
10.0.2.15`. That line is the evidence that Zephyr's in-kernel network stack
and the smsc911x driver came up, so it has to stay Zephyr's.

It is also a DEFERRED log message. With `CONFIG_LOG_MODE_OVERFLOW=y`, a full
log buffer drops the OLDEST pending messages to make room. Issue 2825's
console showed `--- 14 messages dropped ---` during a boot-time warning burst,
no marker, and the image publishing anyway. So the test failed on a healthy
image.

## What has been done, and why it is not the end

- Issue 2815's fix makes the burst quieter: one history-depth warning
  instead of twelve.
- The mps2 board fragment sets `CONFIG_LOG_BUFFER_SIZE=4096`, from 1024.

Both make the drop rarer. Neither makes it impossible: the next burst that
outgrows the buffer drops the marker again, and the test reads as a broken
network stack.

## Directions (not decided)

1. `CONFIG_LOG_MODE_OVERFLOW=n` on the witness boards. A full buffer then
   drops NEW messages, and the marker, logged early, survives. Later
   diagnostics are what gets lost instead.
2. Read the assigned address from the network interface (`net_if`) after
   `net_config` runs, and print it through `printk`, which bypasses the log
   buffer. That keeps the evidence Zephyr's own (it reads the stack's state)
   without riding a droppable channel.
3. Have the harness treat `--- N messages dropped ---` before the marker as
   "marker unknown", not "marker absent", and fall back to a second witness.

## Closes when

A cortex-m cell's readiness verdict cannot be lost to log-buffer overflow,
with a cell that floods the log at boot and still sees the marker.
