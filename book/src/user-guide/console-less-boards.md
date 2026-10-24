# Observing a Board With No Console

Some boards have no console a human can read. On the NXP MR-CANHUBK344 the
console UART is not wired, and the one UART that is carries the zenoh serial
link. A log line written there either goes nowhere or corrupts the transport's
framing. This page lists the channels that still work, in the order to reach
for them.

The order matters. A debug probe is **not** a passive instrument on such a
board (issue 0913): reading memory halts the core, a serial zenoh session
misses its keepalives, and the session does not come back until the router
restarts. So read what the image has already recorded first, and attach a live
reader last.

## 1. The boot record: what the image says about itself

`CONFIG_NROS_BOOT_REPORT=y` keeps one fixed-size record in RAM, the
`NROS_BOOT_REPORT` symbol. The image writes it as it boots, so it survives the
failures that stop a log from ever printing, such as an executor arena that
runs out during entity creation, before the first spin.

It carries:

- the knobs the image was **compiled** with (arena size, callback slots,
  scheduling contexts, nodes, receive buffer);
- the last boot **stage** it reached;
- the allocation that did not fit, by how much, and **which** allocator refused
  it (the executor arena or the platform heap);
- the platform heap's peak (at `FirstSpin` and overall) and its capacity;
- samples dropped for being bigger than their subscription buffer;
- the ROS domain and same-session-query setting, each with the rung of the
  configuration ladder that decided it;
- every `ERROR`/`FATAL` log record, **counted**, with the first one's source
  file and line (issue 1036). This includes errors raised while no console
  exists, such as a full liveliness pool.

Read it after halting the core. The reader takes the address from the image's
own symbol table, so a relinked image cannot be decoded at a stale address:

```sh
read ADDR LEN <<< "$(python3 scripts/read-boot-report.py --addr-only build/zephyr/zephyr.elf)"
pyocd commander -t s32k344 -c halt -c "savemem $ADDR $LEN report.bin"
python3 scripts/read-boot-report.py build/zephyr/zephyr.elf report.bin
```

Halting the core ends a serial zenoh session (the warning above). That does
not matter here, because you read the record **after** the run you care about.
The record is OFF by default on a real board. An image without it is
byte-identical to one built before the feature existed.

## 2. The fault record: what killed it

`CONFIG_NROS_FAULT_LOG=y` replaces Zephyr's fatal-error handler with one that
writes the fault reason, the faulting PC/LR/PSR and the thread **name** into a
`.noinit` record, which survives a functional reset. Set
`CONFIG_THREAD_NAME=y` too, or the thread reads `?`; for a stack overflow, the
thread is the whole question. `CONFIG_NROS_FAULT_LOG_FLASH=y` also copies the
record to the storage partition at the next boot. On the S32K344 a destructive
reset (power-on, pin reset) zeroes the RAM the record lives in, so only a
functional reset preserves it. See the Kconfig help for the limits.

## 3. RTT: a live log, last

When you need the running log, move the console to an **unwired** UART in a
devicetree overlay, so its bytes cannot reach the transport. Then read it over
SEGGER RTT on the SWD link you already have:

```sh
script -qec "pyocd rtt -t s32k344 -a $(arm-none-eabi-nm build/zephyr/zephyr.elf \
    | awk '$3==\"_SEGGER_RTT\"{print \"0x\"$1}')" /dev/null
```

None of the four things this needs is mentioned by any error message:

| Symptom | Fix |
| --- | --- |
| the SEGGER module is not in the workspace | clone `zephyrproject-rtos/segger` (e.g. into `modules/debug/segger`) |
| west does not register it | pass `-DZEPHYR_EXTRA_MODULES=<that path>` |
| `pyocd rtt` needs a TTY | run it under `script -qec "…" /dev/null`, as above |
| it does not find the control block | pass `-a <address of _SEGGER_RTT>`, as above |

The default 1 KiB up-buffer wraps before a reader that attaches after the
event sees anything, so a failing run reads back empty. Set
`CONFIG_SEGGER_RTT_BUFFER_SIZE_UP=16384`.

Everything you observe this way, you observed with the core being halted
underneath it. One crash on the reference island (issue 0852) did not
reproduce once the probe was detached. Check a finding made over RTT again
with the probe detached, using the two records above, before filing it.

## See also

- [Serial Transport](./serial-transport.md), including the two settings a
  Zephyr board joining over a UART needs.
- [Logging](./logging.md), for the log levels the boot record counts.
- `scripts/read-boot-report.py --help`, for the heap-headroom gate and the
  field list.
