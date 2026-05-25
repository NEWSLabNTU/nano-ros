# Zephyr 3.7 → 4.4 divergence audit (Phase 180.A.0)

**Purpose.** Scope what changes for nano-ros when its Zephyr module must build
on both Zephyr 3.7 LTS (current pin) and 4.4 (latest rolling). Input for the
Phase 180.A.1+ implementation plan. Research only — no 4.4 workspace was built;
findings are from the module/patch dependency surface, the 4.0–4.4 migration
guides, and direct reads of v4.4.0 source for the high-risk internal files.

**Date.** 2026-05-25. **Branch.** `phase-180a-version-spanning-module`.

## Method

1. Enumerated the Zephyr API + file surface nano-ros depends on
   (`packages/core/nros-platform-zephyr/`, `zephyr/*.c`, and the 16
   `scripts/zephyr/*-patch.sh`).
2. Cross-referenced the cumulative migration guides 4.0 → 4.4.
3. For the two highest-risk patch families (NSOS, Rust) read v4.4.0 source
   directly, since those patch *internal* files migration guides do not track.

## Dependency surface

**Module runtime APIs** (`nros-platform-zephyr`): heavy `zsock_*` BSD-socket
use (`zsock_socket/bind/setsockopt/getaddrinfo/recvfrom/sendto/fcntl/
shutdown/close`, `zsock_addrinfo`), core kernel `k_*` (timer, sem, malloc/
free, sleep/yield, uptime), `sys_rand_get`, `net_if`. Kernel `k_*` is stable
across the jump; risk concentrates in the socket/POSIX/native_sim layers.

**16 patch scripts, by target:**
- **NSOS** (`drivers/net/nsos_*.c/h`) — 6: `native-sim-ipproto-ip`,
  `nsos-adapt-ipproto-ip`, `nsos-mcjoin-mreq`, `nsos-getsockname`,
  `nsos-getifaddrs`, `nsos-recvmsg`. native_sim only (the example test target).
- **Rust glue** (`modules/lang/rust/CMakeLists.txt`) — 4: `aarch64-rust`,
  `cortex-a9-rust`, `cortex-r-rust`, `cargo-features`.
- **Cyclone-on-Zephyr** — 5: four patch the **cyclonedds submodule**
  (`src/ddsrt/...`: log-flush, mcjoin, sockwaitset, udp-rcvbuf — Zephyr-version
  *independent*), one patches Zephyr POSIX (`cyclonedds-zephyr-threads` →
  `lib/posix/options/signal.c`).
- **Build** — `llext-edk-conditional` (WORKSPACE-conditional build gate).

## Findings

### NSOS (drivers/net/nsos_*) — empirically checked against v4.4.0

| Capability the patch adds | v4.4.0 upstream state | Fate on 4.4 |
| --- | --- | --- |
| `getsockname` | **present** (`nsos_getsockname` in vtable + `nsos_adapt_getsockname`) | **obsolete — drop** |
| `recvmsg` | present but a stub returning `ENOTSUP` | **reshape** — fill the stub rather than add the function |
| IP multicast `IP_ADD_MEMBERSHIP`/`IP_DROP_MEMBERSHIP` (`IPPROTO_IP`) | **absent** — `nsos_setsockopt`/`nsos_adapt_setsockopt` handle only SOL_SOCKET / TCP / IPV6 | **still needed**, re-anchor to 4.4 switch shape (×3) |
| `getifaddrs` | **absent** | **still needed**, re-anchor |

Net: 6 NSOS patches → **1 dropped, 1 reshaped, 4 re-anchored**. The 4.4
`setsockopt` switch grew an IPV6 level but keeps the same protocol-level switch
structure, so re-anchoring the IP-multicast patches is mechanical, not a
rewrite. getsockname landing upstream confirms these are **upstreamable** —
feed the IP-multicast / getifaddrs / recvmsg work upstream (Phase 180.D) to
retire them on the 4.x line entirely.

### Sockets / native_sim — migration-guide confirmed
- `native_posix` deprecated → `native_sim` (**already used** ✓).
- `CONFIG_NATIVE_APPLICATION` deprecated → native_simulator runner;
  `CONFIG_NATIVE_SIM_NATIVE_POSIX_COMPAT` now defaults `n` and is deprecated —
  **audit `examples/zephyr/*/boards/native_sim*.conf` for reliance.**
- `CONFIG_NET_SOCKETS_POLL_MAX` → `CONFIG_ZVFS_POLL_MAX`; sockets now route
  through the new ZVFS layer. `zsock_*` call sites are unaffected; any
  poll-max / fd-table Kconfig in overlays needs the rename.
- Header move `include/zephyr/net/buf.h` → `include/zephyr/net_buf.h` — grep
  module + examples.

### POSIX / pthread Kconfig — NEEDS VERIFICATION against live 4.4 Kconfig
Overlays use `CONFIG_POSIX_API`, `CONFIG_MAX_PTHREAD_MUTEX_COUNT`,
`CONFIG_MAX_PTHREAD_COND_COUNT`, `CONFIG_MAX_PTHREAD_COUNT`,
`CONFIG_POSIX_THREAD_THREADS_MAX`. The 4.0/4.1 migration guides fetched did
not surface a POSIX rename, but the POSIX subsystem granularized across the
4.x line (per-option `CONFIG_POSIX_*`). The `CONFIG_MAX_PTHREAD_*` family in
particular is a rename risk. **Resolve by diffing these symbols against a 4.4
`menuconfig`/Kconfig once a 4.4 tree exists** (one of the first 180.A.1 tasks).

### Rust module (modules/lang/rust) — NEEDS VERIFICATION against live tree
Rust became an **official** Zephyr module in 4.1; `west.yml` floats
`zephyr-lang-rust` at `main`. The 4 Rust patches sed-inject cargo-profile
logic into `modules/lang/rust/CMakeLists.txt` by grepping for a sentinel
string. High churn risk — the official module's CMake shape differs from the
pinned 3.7-era checkout. **Pin `zephyr-lang-rust` to the rev matching each
Zephyr line and re-verify all 4 anchors against it.** Likely the largest
single chunk of 180.A.

**RESOLVED (Task 9, 2026-05-25).** Verified against the 4.4-cloned
`zephyr-lang-rust` (pinned to `a763400f` in `west-4.4.yml`). Findings:
`_rust_map_target` already maps `ARCH_POSIX + 64BIT + x86_64/aarch64` host
→ `x86_64/aarch64-unknown-none`, so **native_sim is upstream-supported and
needs none of the patches**. The 3 arch patches (aarch64 / cortex-a9 /
cortex-r) are **still needed but re-anchorable** — the `_rust_map_target`
`elseif(CONFIG_CPU_*)` chain has the same shape as 3.7 and still lacks the
AArch64-baremetal / Cortex-A / Cortex-R branches (grep=0); they travel with
the FVP / S32Z / Zynq board targets, not native_sim. `cargo-features`
re-anchors into the still-present `rust_cargo_application()`. **None
obsolete; all re-anchorable; risk downgraded High → Low/Medium.** The "largest
chunk" framing was wrong — the official-module shape is conceptually
unchanged from the 3.7-era checkout.

### Build / libc — low risk
- HWMv1 removed in 4.2 → any nano-ros-contributed boards (`board_root`,
  Phase 180.C) must be HWMv2. native_sim is already HWMv2; current overlays
  are app-level, not board defs.
- `llext-edk-conditional` patch is a local build gate; re-test, no documented
  upstream conflict.
- libc Kconfig (`CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE`, `CONFIG_PICOLIBC_*`)
  showed no migration-guide rename; verify against 4.4 when the tree exists.
- Cyclone submodule patches are Zephyr-version-independent (depend on the
  cyclonedds tag); re-test but expect no Zephyr-bump impact.

## Risk tiers

- **High (re-verify against a live 4.4 tree):** Rust module CMake (4 patches),
  POSIX/pthread Kconfig renames.
- **Medium (bounded, re-anchor):** NSOS IP-multicast + getifaddrs + recvmsg
  (5 patches after dropping getsockname), ZVFS poll-max rename.
- **Low (re-test, no expected break):** kernel `k_*` APIs, `zsock_*` call
  sites, cyclone submodule patches, llext-edk, libc Kconfig.

## Conclusion

The jump is **bounded and partly self-reducing** — one NSOS patch is already
obsolete on 4.4, and the rest are mechanical re-anchors or upstreaming
candidates. The two items that genuinely need a live 4.4 tree before they can
be planned with no placeholders are the **Rust module** re-verification and the
**POSIX Kconfig** rename audit. Those become the first concrete 180.A.1 tasks
(stand up a 4.4 workspace, then resolve them), and the rest of 180.A.1+ can be
written from this audit.

## Sources
- Zephyr v4.4.0 NSOS source: `drivers/net/nsos_sockets.c`, `drivers/net/nsos_adapt.c`
  (github.com/zephyrproject-rtos/zephyr @ v4.4.0)
- Migration guides 4.0–4.2 (docs.zephyrproject.org / repo `doc/releases/`)
- Local: `scripts/zephyr/*-patch.sh`, `packages/core/nros-platform-zephyr/`
