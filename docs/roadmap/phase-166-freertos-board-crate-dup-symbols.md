# Phase 166 — Duplicate `nros_platform_*` symbols across FreeRTOS board crates

**Goal.** Eliminate the duplicate-symbol linker error when both
`nros-board-freertos` (common FreeRTOS overlay) and a board-specific
overlay (`nros-board-mps2-an385-freertos`, eventual STM32F4 / NXP /
TI variants) are pulled into the same binary. The `platform.c` C
body that exports the canonical `nros_platform_*` ABI is being
compiled twice with non-weak linkage.

**Status.** Not Started.

**Priority.** P1 — blocks `just freertos build-fixtures` for the
Rust DDS example today, and will block every future board crate
that layers on top of `nros-board-freertos`.

**Depends on.** Nothing.

---

## Symptom

```
$ just freertos build-fixtures
…
rust-lld: error: duplicate symbol: nros_platform_clock_ms
>>> defined at platform.c
>>>            91eeb4584ba792b3-platform.o:(nros_platform_clock_ms)
       in archive .../libnros_board_mps2_an385_freertos-…rlib
>>> defined at platform.c
>>>            91eeb4584ba792b3-platform.o:(nros_platform_clock_ms)
       in archive .../libnros_board_freertos-…rlib

rust-lld: error: duplicate symbol: nros_platform_task_init
>>> …
… (~20 more `nros_platform_*` symbols)
…
error: could not compile `qemu-freertos-dds-listener`
       (bin "qemu-freertos-dds-listener")
```

## Root cause

Both crates compile `packages/core/nros-platform-freertos/src/platform.c`
into their respective rlibs. Each definition exports the
`nros_platform_*` C ABI without `weak` linkage, so when the linker
walks both rlibs it sees two strong defs for every function.

- `packages/boards/nros-board-freertos/build.rs` invokes the
  platform compile.
- `packages/boards/nros-board-mps2-an385-freertos/` depends on
  `nros-board-freertos` AND also compiles `platform.c` (via its
  Cargo dep on `nros-platform-freertos` whose `build.rs` rebuilds
  it).

End state: two strong defs reach the linker, rust-lld refuses to
pick one.

## Fix options

Pick exactly one:

1. **Only the board-specific overlay emits the C body.**
   `nros-board-freertos` stops compiling `platform.c`; the
   board-specific crate that depends on it picks the compile up.
   Pro: minimal, board-specific build steps already exist.
   Con: every future board crate has to remember to compile it.

2. **Only `nros-platform-freertos` emits the C body.**
   `nros-board-freertos` stops compiling `platform.c` AND
   `nros-board-mps2-an385-freertos` does NOT add it either.
   `nros-platform-freertos`'s own `build.rs` produces a staticlib
   that every consumer links against.
   Pro: canonical — one platform crate, one C body.
   Con: needs build-script reshape to actually emit a staticlib,
   not just a `cc::Build` invocation.

3. **Gate `platform.c` emission behind a Cargo feature.**
   `nros-platform-freertos` exposes `emit-c-port` (default `on`).
   Board overlays that want to emit themselves opt out via
   `default-features = false`.
   Pro: flexible — opt-in / opt-out per consumer.
   Con: another feature axis to remember; misconfiguration leaves
   `undefined-symbol` instead of `duplicate-symbol`.

Recommend option **2** (canonical platform crate emits, board crates
consume). Matches the platform-cffi pattern documented in
`book/src/internals/platform-c-abi.md`.

## Work items

- [ ] **166.1** Audit which crates currently compile `platform.c`:
      - `nros-platform-freertos/build.rs` or CMakeLists.txt
      - `nros-board-freertos/build.rs`
      - `nros-board-mps2-an385-freertos/build.rs` (or Cargo.toml
        feature inheritance)
- [ ] **166.2** Pick the fix option (recommend option 2). Land the
      build-script reshape on a feature branch.
- [ ] **166.3** Verify with `just freertos build-fixtures` that
      both Rust zenoh + Rust DDS examples build cleanly. Verify
      C / C++ examples still link.
- [ ] **166.4** Repeat for any other RTOS that has a board-overlay
      pattern (Zephyr `nros-board-fvp-aemv8r-smp` over a generic
      Zephyr platform crate, NuttX QEMU board crates, etc.). Audit
      whether the same dup-symbol risk exists.
- [ ] **166.5** Regression test in `nros-tests`: build every
      board crate against every supported example tree as part of
      `just test-all`, asserting clean link.

## Files (likely touched)

- `packages/core/nros-platform-freertos/build.rs` (new — turn
  current `cc::Build` into a staticlib emission)
- `packages/boards/nros-board-freertos/build.rs` (drop the
  platform.c compile)
- `packages/boards/nros-board-mps2-an385-freertos/Cargo.toml`
  (confirm it pulls in the staticlib via the platform crate, not
  via its own compile)

## Acceptance criteria

- [ ] `just freertos build-fixtures` runs clean — all 20 binaries
      (rust + c + cpp × {pubsub, service, action} + DDS pair) build.
- [ ] `cargo build` from each `examples/qemu-arm-freertos/<lang>/<rmw>/<ex>/`
      links without `duplicate symbol` errors.
- [ ] `nm libnros_board_mps2_an385_freertos-*.rlib` shows zero
      `nros_platform_*` symbols defined in the rlib (they should
      resolve from the platform crate's staticlib).
- [ ] No regression on other RTOS targets that exercise the same
      board-overlay pattern.

## Notes

- This regression most likely landed during one of the Phase
  121/129 platform-cffi reshape commits. Surfaced by the user-as-
  tester agent against the FreeRTOS starter on 2026-05-19.
- The symptom is masked when only ONE board crate participates
  in a binary (the Rust zenoh examples that bypass
  `nros-board-freertos` still build cleanly because they only
  pull in `nros-board-mps2-an385-freertos`).

---

## Non-passing test inventory (snapshot 2026-05-19)

Cataloged during the Phase 88.16.B verification sweep. Pulled from a
full grep of `packages/testing/nros-tests/tests/*.rs`. Two classes:
hard-coded `#[ignore]` markers (test runner reports `ignored`) and
prerequisite skips through `nros_tests::skip!(...)` (test runner
reports `[SKIPPED]` via panic+prefix). None of these are caused by
Phase 88 / nros-log; they predate it.

### Permanently `#[ignore]`'d — needs upstream / other-phase fix

| Test | Reason | Tracking |
|---|---|---|
| `actions::test_action_server_client_communication` | blocking `zpico_get` in `send_goal` returns `Timeout` immediately on native | Phase 77 |
| `native_api::test_c_action_communication` | same root cause | Phase 77 |
| `native_api::test_c_rust_service_interop` | blocking `zpico_get` in service call returns `Timeout` | Phase 77 |
| `nuttx_qemu::test_nuttx_cpp_talker_builds` | NuttX C/C++ CMake build blocked by upstream libc missing `_SC_HOST_NAME_MAX` | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_listener_builds` | same | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_service_server_builds` | same | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_service_client_builds` | same | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_action_server_builds` | same | NuttX upstream |
| `nuttx_qemu::test_nuttx_cpp_action_client_builds` | same | NuttX upstream |
| `esp32_qemu_dds::test_esp32_qemu_dds_rust_talker_to_listener_e2e` | dust-dds `DcpsDomainParticipant` builtin entity count overflows ESP32-C3 heap budget; Phase 101 deferral | Phase 101 follow-up |
| `freertos_qemu_dds::test_freertos_dds_rust_talker_to_listener_e2e` | gates flipping 97.4.freertos to done; runtime smoke deferred | Phase 97.4 |

**Phase 77 status:** "In Progress (77.1–77.5 done)" per
`docs/roadmap/archived/phase-77-async-action-client.md`. Three
`#[ignore]` markers above clear when 77.6+ lands.

### Prerequisite-gated `nros_tests::skip!(...)` — environmental

These pass when the listed dependency is installed / running; they
print `[SKIPPED] <reason>` and exit otherwise. Skip frequency
across the suite (full grep, with-duplicates):

| Skip reason | Count | Unblocker |
|---|---|---|
| zenohd not found | 78 | `just zenohd build` (artefact at `build/zenohd/zenohd`) |
| Zephyr not available | 49 | `just zephyr setup` |
| XRCE agent not available | 33 | `just xrce setup` |
| ROS 2 not found | 20 | `source /opt/ros/humble/setup.bash` |
| cmake not found | 12 | distro-level `cmake` install |
| zenoh-pico arm build not available | 10 | `just qemu build-zenoh-pico-arm` |
| `require_nuttx_cpp` check failed | 6 | NuttX C/C++ block (see above) |
| ROS 2 DDS not available | 5 | ROS 2 install + rmw_cyclonedds_cpp |
| DDS talker / listener binary missing | 10 | `just <plat> build-fixtures` |
| west command not available | 4 | `pip install west` |
| socat not available | 3 | distro `socat` install |
| riscv32 target not available | 3 | `rustup target add riscv32imc-unknown-none-elf` |
| `require_esp32_networked` check failed | 3 | `just esp32 setup` |
| qemu-system-arm too old for `-netdev dgram unix` | 2 | `just qemu setup-qemu` (patched binary) |
| Patched qemu-system-arm not built | 2 | `just qemu setup-qemu` |
| ThreadX (Linux + RV64) DDS prereq | 2 | `just threadx_{linux,riscv64} setup` |
| `require_threadx{,_riscv64}` check failed | 2 | same |
| `require_nuttx` / `require_freertos` check failed | 2 | `just {nuttx,freertos} setup` |
| `qemu-system-riscv32` not available | 1 | distro `qemu-system-misc` install |
| `pio` CLI not on PATH | 1 | `pip install platformio` |
| Phase 138.6 zephyr cell deferred | 1 | Phase 139 work |
| openssl not available — cannot generate TLS certs | 1 | distro `openssl` install |
| NUTTX_DIR unset | 1 | env var |
| `idf.py` not on PATH | 1 | `just esp_idf setup` |
| `espflash` not available | 1 | `cargo install espflash` |
| `arm-none-eabi-gcc` not on PATH | 1 | distro `gcc-arm-none-eabi` install |
| bare-metal DDS prerequisites not available | 1 | `just qemu setup` |
| zenoh-pico arm build (bridge variant) | 1 | `just qemu build-zenoh-pico-arm` |

### Pure-fixture skip (separate failure mode)

`xrce::test_xrce_large_message_publish` fails (not skip!) with
`Test fixture binary not prebuilt: .../target/release/xrce-large-msg-test`.
Resolves with `just build-test-fixtures`. The xrce harness panics
on the missing path because the test was written before the
`skip!` macro existed.

### Survey gap — broader ignored / skipped set this snapshot
misses

This inventory was gathered from a static grep, not from a full
`just test-all` run. Real run-time counts vary by:
- which optional services are running (zenohd, xrce-agent),
- which platform toolchains are installed,
- which fixtures are prebuilt (`just build-test-fixtures`).

Run `cargo nextest run --workspace 2>&1 | tee tmp/nextest.log` and
grep for `SKIPPED` + `ignored` to refresh this snapshot before
treating it as canonical.

### Recommended dispositions

- **Phase 77 trio (3 tests)** — keep ignored; resolution gated on
  Phase 77.6+.
- **NuttX C/C++ block (6 tests)** — keep ignored; resolution
  gated on the upstream NuttX libc patch.
- **DDS RTOS smoke (2 tests)** — keep ignored; Phase 97.4 /
  Phase 101 cleanups; un-skip when their respective work items
  flip to done.
- **Environmental skips (77 + 49 + …)** — leave as is; these are
  desirable (CI / dev hosts without the dep should skip cleanly).
- **xrce large-message fixture** — convert from raw panic to
  `nros_tests::skip!("xrce-large-msg-test fixture not prebuilt; run
  `just build-test-fixtures`")` so the test reports `[SKIPPED]`
  instead of `FAILED` when fixture is missing. Trivial follow-up.
