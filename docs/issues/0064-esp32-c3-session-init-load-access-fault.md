---
id: 64
title: esp32-c3 QEMU — Load-access-fault (mtval=0xffffffff) in zenoh-pico config init crashes session bring-up
status: open
type: bug
area: platform
related: [phase-248, phase-249]
---

## Symptom

The networked `esp32_emulator` live tests are red:
`test_esp32_talker_listener_e2e`, `test_esp32_to_native`, `test_native_to_esp32`,
`test_esp32_workspace_entry_e2e` (4/8 of the file; the build/detection 4 pass).
Intermittent — a node sometimes connects, sometimes the firmware faults.

## Real cause (re-diagnosed 2026-06-15)

The prior `phase-89.4-followup` TODO ("OpenETH smoltcp never emits the final ACK /
handshake stalls / `Transport(ConnectionFailed)`") is **stale**. With a full QEMU
backtrace the listener reaches `Waiting for messages...` — `Executor::open` +
subscribe succeed, so TCP + the zenoh session open work. The real failure is a
firmware CPU exception during session **init**:

```
Exception 'Load access fault' mepc=0x4203e302, mtval=0xffffffff
  libc_stubs::strlen                         (esp32-qemu platform, the faulting load)
  <- zenoh-pico _z_str_size / _z_str_clone    (collections/string.c:165 / :189)
  <- _zp_config_insert                        (protocol/config.c:36)
  <- zpico_init_with_config                   (zpico-sys/c/zpico/zpico.c:833)
  <- nros_rmw_zenoh::zpico::Context::with_config (nros-rmw-zenoh/src/zpico.rs:347)
```

`mtval=0xffffffff` is a deref of an all-ones **pointer value** (not a walk-off-end:
a valid esp32 DRAM/flash string ptr that walked off would fault near the segment
end ~0x3fce0000, not 0xffffffff). So a config-string **value** handed to
zenoh-pico's config intmap is the literal pointer `0xffffffff`.

esp32-c3 (QEMU OpenETH) **only** — the identical `with_config` path is runtime-green
on freertos / threadx / native. So it is memory corruption local to the bare-metal
esp32 session-init path, NOT networking.

## Ruled out (static analysis)

- **Stale global `g_config`** — `zpico_init_with_config` runs `z_config_default(&g_config)`
  every call (zpico.c:770).
- **Non-NUL-terminated locator/property values** — all are NUL-terminated stack
  buffers in `SmoltcpSession::new` (`shim/session.rs`); `c_props` is zeroed and only
  `&c_props[..prop_count]` (valid entries) is passed; the property loop guards NULL.
- **Dangling stack pointers from retry** — `connect_with_retry` (zpico.rs:275) loops
  **synchronously** with backoff inside `with_config`'s scope; the captured buffers
  stay live.
- **Too-small main stack** — ~18 KB (`_stack_start` 0x3fcce400 − `_stack_end`
  0x3fcc9a4c); the ~4.2 KB `SmoltcpSession::new` frame (key_bufs/val_bufs 2×256×8) is
  large but the fault signature is a bad pointer *value*, not a stack-guard hit.

## Open leads

- `connect_with_retry` re-invoking `zpico_init_with_config` over shared globals
  (re-entrancy / partial-init state).
- esp32 heap corruption from `z_malloc` in the config-clone path (`_z_str_clone`).
- The intermittency suggests an uninitialised value or a race with the zenoh-pico
  read/lease tasks.

## Next step

Pin *which* insert's value is `0xffffffff` (mode / auto `session_zid` / a property /
the locator). esp32 bare-metal has **no libc `printf`** (a C-printf probe fails to
link — the firmware prints via Rust `esp_println`), so instrument via either a
printf-free guard returning distinct error codes per insert site, or **gdb on the
QEMU gdbstub** (`-S -gdb tcp::1234`, `riscv32` gdb, break in `_z_str_clone` /
`_zp_config_insert`, inspect the arg). Then fix the corruption source.

## Fixed alongside (this issue's commit `651f7f579`)

- Replaced the stale OpenETH TODO in `esp32_emulator.rs` with the above diagnosis.
- `just esp32 build-fixtures` now stages the `esp32_entry` workspace fixture (was
  only built by `build-examples`, though `test_esp32_workspace_entry_e2e`'s skip
  message points at `build-fixtures`).
