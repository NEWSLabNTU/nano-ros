# nros-platform-zephyr

Zephyr platform implementation for nano-ros. Sits on top of Zephyr's
POSIX layer and native socket API.

## Role

Implements the trait family in
[`nros-platform-api`](../nros-platform-api) on top of Zephyr:
`k_uptime_get` for monotonic time, `k_malloc` / `k_free` for heap,
Zephyr POSIX pthreads + mutexes + condvars for threading, Zephyr POSIX
sockets for networking. Build-time wiring goes through the Zephyr
module at the project root (`zephyr/`).

## Source layout

| File | Role |
|------|------|
| `src/lib.rs` | `ZephyrPlatform` zero-sized type + trait impls. |
| `src/ffi.rs` | `printk` wrappers + Zephyr native FFI imports. |
| `src/net.rs` | TCP / UDP / multicast over Zephyr POSIX sockets (or NSOS on `native_sim`). |
| `../../../zephyr/` | Zephyr module manifest, Kconfig, CMakeLists.txt, C shims. |

## When to use

- Any Zephyr-supported board (real or `native_sim` / `native_posix`).
- Build via `west build` against the in-tree Zephyr module.

## Caveats

- POSIX mutex / condvar pool defaults are **too low** for zenoh-pico —
  set `CONFIG_MAX_PTHREAD_MUTEX_COUNT=32` and
  `CONFIG_MAX_PTHREAD_COND_COUNT=16` in the board's `prj.conf`. See the
  CLAUDE.md "Zephyr POSIX Resource Limits" section for the failure mode.
- `native_sim` multicast requires the NSOS `IPPROTO_IP` patch
  (`scripts/zephyr/native-sim-ipproto-ip-patch.sh`, run automatically by
  `just zephyr setup` / `build-fixtures`).
- Service / action client paths need
  `CONFIG_NATIVE_SIM_SLOWDOWN_TO_REAL_TIME=y` on `native_sim` to keep
  the QEMU virtual clock from racing past the test fixtures.

## See also

- [Custom Platform porting guide](../../../book/src/porting/custom-platform.md)
- CLAUDE.md "Zephyr POSIX Resource Limits" + "QEMU Clock Synchronization"
- Source on GitHub: <https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/core/nros-platform-zephyr>
