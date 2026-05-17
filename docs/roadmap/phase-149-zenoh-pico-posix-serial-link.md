# Phase 149 — Zenoh-Pico POSIX Serial Link Gap

**Goal.** Provide impls (stub or functional) for the
`_z_*_serial_*` symbols that `src/link/unicast/serial.c` and
`src/system/common/serial.c` reference on POSIX. Today the
wrappers compile (Phase 136 manifest enables them by default or
zenoh-pico's CMake default does) but the underlying
`_z_open_serial_from_pins`, `_z_open_serial_from_dev`,
`_z_listen_serial_from_pins`, `_z_listen_serial_from_dev`,
`_z_close_serial`, `_z_send_serial_internal`, `_z_read_serial_internal`
are missing → every C/C++ example linking the POSIX zenoh staticlib
fails at the final link with ~12 undefined references.

**Status.** Not started.

**Priority.** P1 — blocks every C/C++ POSIX example link path. 58
of the 144 post-Phase-140 test-all failures (native_api action /
service / talker / listener builds) are this single root cause.

**Depends on.** None. Self-contained POSIX zenoh-pico aliasing.

**Related.** Phase 134 (canonical UDP-multicast version of this
class — stub aliases in platform_aliases.c), Phase 146 (closed the
embedded RTOS link regressions; POSIX serial wasn't in scope),
Phase 140 (CI run that surfaced this post-install-local-rip-off).

---

## Symptom

```
/usr/bin/ld: libnros_rmw_zenoh_staticlib.a(serial.c.o): in function
  `_z_open_link_serial_from_pins':
.../zenoh-pico/src/link/unicast/serial.c:82: undefined reference to
  `_z_open_serial_from_pins'
... (+11 similar)
collect2: error: ld returned 1 exit status
```

Surfaces in every C/C++ example final-link step that pulls
`libnros_rmw_zenoh_staticlib.a` (whole-archive). `cargo check`
doesn't catch this — link errors only fire at executable link
time.

## Root cause

Same shape as Phase 134's UDP-multicast bug:

1. `Z_FEATURE_LINK_SERIAL` is enabled (either by zenoh-pico's CMake
   default or by Phase 136 manifest path).
2. `src/link/unicast/serial.c` + `src/system/common/serial.c` get
   compiled, both gated by `#if Z_FEATURE_LINK_SERIAL == 1`.
3. They call `_z_open_serial_from_*`, `_z_send_serial_internal`,
   `_z_read_serial_internal` — implementations live in
   `src/system/<platform>/serial.c`.
4. `build_zenoh_pico_unified` POSIX path doesn't compile a POSIX
   `serial.c` (zenoh-pico ships ESP-IDF / FreeRTOS / Zephyr serial
   impls but no `unix/serial.c`).
5. `packages/zpico/zpico-sys/c/zpico/platform_aliases.c` (Phase
   128.D.3) provides `z_*` / `_z_*` aliases for TCP, UDP-unicast,
   UDP-multicast (Phase 134), but never grew matching `_z_*_serial*`
   aliases.

## Fix options

### A. Stub aliases (Phase 134's recipe)

Add 7 stubs to `platform_aliases.c`:

```c
int8_t _z_open_serial_from_pins(void *sock, uint32_t txpin,
                                uint32_t rxpin, uint32_t baudrate) {
    (void)sock; (void)txpin; (void)rxpin; (void)baudrate;
    return -1;
}
int8_t _z_open_serial_from_dev(void *sock, char *dev, uint32_t baudrate) {
    (void)sock; (void)dev; (void)baudrate;
    return -1;
}
int8_t _z_listen_serial_from_pins(void *sock, uint32_t txpin,
                                  uint32_t rxpin, uint32_t baudrate) {
    (void)sock; (void)txpin; (void)rxpin; (void)baudrate;
    return -1;
}
int8_t _z_listen_serial_from_dev(void *sock, char *dev, uint32_t baudrate) {
    (void)sock; (void)dev; (void)baudrate;
    return -1;
}
void _z_close_serial(void *sock) { (void)sock; }
size_t _z_send_serial_internal(void *sock, const uint8_t *buf, size_t len) {
    (void)sock; (void)buf; (void)len;
    return 0;
}
size_t _z_read_serial_internal(void *sock, uint8_t *buf, size_t len) {
    (void)sock; (void)buf; (void)len;
    return 0;
}
```

Pro: mirrors Phase 134's UDP-multicast resolution. Archive links
cleanly; serial transport reachable but non-functional. POSIX
discovery via TCP/UDP (rmw_zenoh default) unaffected.
Con: serial transport doesn't actually work on POSIX (was never
expected to).

### B. Functional POSIX serial via termios

Implement `_z_open_serial_from_dev` with `open(O_RDWR | O_NOCTTY)`,
`tcgetattr` / `tcsetattr` for baud + 8N1 setup. Stub the
`from_pins` variants (POSIX has no pin-level UART access).

Pro: serial actually works on POSIX (useful for embedded gateway
scenarios, RPi USB-serial dev boxes).
Con: ~150 LOC of POSIX serial code; nobody's asked for it.

### C. Compile-out `Z_FEATURE_LINK_SERIAL` on POSIX

Pass `Z_FEATURE_LINK_SERIAL=0` for POSIX in
`packages/zpico/zpico-sys/zenoh_platforms.toml`'s POSIX entry.
zenoh-pico's `serial.c` files get gated out entirely; no
references generated → no missing symbols.

Pro: minimum-deletion. Truly nothing-to-link.
Con: changes the runtime feature surface for POSIX consumers;
anyone enabling `link-serial` Cargo feature on POSIX hits an
unhelpful build error instead of a no-op runtime.

**Recommend A** — matches Phase 134's proven pattern, preserves
the runtime feature surface (user can call the alias and get a
proper error code), minimum churn.

---

## Work Items

- [ ] **149.1 — Add 7 serial alias stubs to platform_aliases.c.**
      Mirror Phase 134's UDP-multicast stub block. Each returns
      `-1` / `0`. Comment block matches Phase 134's voice.
      **Files.** `packages/zpico/zpico-sys/c/zpico/platform_aliases.c`.

- [ ] **149.2 — Update `scripts/check-zenoh-archive-symbols.sh`
      (Phase 134.4) to assert serial wrapper / impl pair-match.**
      Catches the regression class permanently. Same shape as the
      existing multicast / unicast / tcp checks.
      **Files.** `scripts/check-zenoh-archive-symbols.sh`.

- [ ] **149.3 — Smoke verify against the failing test class.**
      ```bash
      cd examples/native/c/zenoh/action-server
      rm -rf build && cmake -S . -B build && cmake --build build
      ```
      Was failing pre-149 with the listed undefined references;
      post-149 must link clean.
      **Files.** none (verification).

- [ ] **149.4 — Re-run `just ci` to confirm the 58 native_api
      action/service failures drop.** Expected post-149:
      `just test-all` failure count drops by ~58.
      **Files.** none (verification).

---

## Acceptance

- [ ] `cargo check` + `cmake --build` on every
      `examples/native/{c,cpp}/zenoh/{action-server,action-client,service-server,service-client,talker,listener}/`
      succeeds via add_subdirectory path — no undefined references.
- [ ] `scripts/check-zenoh-archive-symbols.sh` includes serial-pair
      assertion; runs green.
- [ ] `just ci` test-all failure count drops by ≥58 (native_api
      action/service/build classes).
- [ ] No regression in already-passing classes.

---

## Notes

- **Why not earlier.** Phase 134 only audited UDP-multicast because
  that was the loudly-failing class at the time. Serial was a
  silent failure (no consumer was building C/C++ POSIX examples
  end-to-end through the legacy install path, because Phase 144's
  `add_subdirectory` migration was the first time the link surface
  got exercised broadly). Phase 140's `install-local` rip-off + the
  fixture migration to add_subdirectory consumption FINALLY ran
  the link for every example — and the serial gap fell out.
- **Why P1.** 58 native_api tests fail on this single root cause.
  Quick stub fix. Highest ROI of any open phase right now.
- **Defense in depth.** 149.2 extends the existing Phase 134.4
  symbol-pair gate. Future serial-feature work surfaces in CI, not
  in a user's link error.
