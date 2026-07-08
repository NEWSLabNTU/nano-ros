---
id: 163
title: "Pure-Rust Zephyr images carry NO zenoh/xrce backend — `nros_rmw_{zenoh,xrce}_register` undefined (only cyclonedds works)"
status: resolved
type: bug
area: zephyr
related: [issue-0155, issue-0161, phase-248, phase-249]
---

## RESOLVED (2026-07-09) — option 1 landed; full pure-Rust Zephyr matrix green

Landed the nros-c-parity shape plus three stacked fixes the lane needed once
the backend actually rode in the image:

1. **Real backend deps** (6 example apps): `rmw-zenoh = ["dep:nros-rmw-zenoh"]`
   (features `platform-zephyr` → `zpico-sys/zephyr`; the module still compiles
   `zpico.c` itself — the same split `libnros_c.a` uses) and
   `rmw-xrce = ["dep:nros-rmw-xrce-cffi"]`; `.cargo/config.toml` patch rows
   added. cyclonedds stays inert (module C++ lib provides the symbol).
2. **Force-link in the macro**: rustc's staticlib DCE drops an unreferenced
   dep's `#[no_mangle]` export (nm: symbol in the rlib, absent from
   `librustapp.a` — exactly what nros-c's `FORCE_LINK` anchor documents).
   `nros::zephyr_component_main!` now makes cfg-gated direct
   `nros_rmw_{zenoh,xrce_cffi}::register()` calls — the reference IS the
   force-link, idempotent with the `nros_app_register_backends` hook.
3. **Right allocator knob**: the Zephyr Rust global allocator is picolibc
   malloc, so phase-271's ~75 KB executor backing OOM'd the DEFAULT 16 KB
   `COMMON_LIBC_MALLOC_ARENA_SIZE` — `CONFIG_HEAP_MEM_POOL_SIZE` (which #155
   bumped) was the wrong knob for this path. prj-{zenoh,xrce}.conf now set a
   1 MB arena (mirrors prj-cyclonedds.conf's).
4. **XRCE locator bake**: nothing consumed `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}`
   on the Rust path; the example build.rs now synthesizes `host:port` into the
   same `NROS_LOCATOR` env the macro reads when `CONFIG_NROS_RMW_XRCE=y`.

Proof: rust zenoh pubsub/service/action/to-native/native-client + rust xrce
talker-listener/service/action ALL PASS (the zenoh lane's first green since
the phase-248/249 rework); phase_118 stays 8/8 ×3 parallel. The weak-guarded
stub stays (an image with a misconfigured backend still fails loudly at open
rather than at link). Remaining zephyr.rs failures are #164's other
categories (stale markers, C/C++ xrce, cyclone action row, workspace entry).

## Summary

A pure-Rust Zephyr example image (`build-rs-<case>-{zenoh,xrce}`,
`nros::zephyr_component_main!` apps) contains **no RMW backend at all** for
zenoh and xrce:

- The app crate's `rmw-zenoh`/`rmw-xrce` cargo features are **inert markers**
  (#60 T5 / phase-248 C6g): they no longer pull `nros-rmw-zenoh` /
  `nros-rmw-xrce-cffi`, on the theory that "the Zephyr board's CMake C-port
  links the concrete backend".
- That theory holds only for **cyclonedds** (the module's C++ lib defines
  `nros_rmw_cyclonedds_register`). For **zenoh** the vtable is Rust-side
  (`RustBackendAdapter<ZenohRmw>` in `nros-rmw-zenoh`) — the module compiles
  `zpico.c` (the transport) but nothing that registers a CFFI vtable. For
  **xrce** the C vtable TU (`nros-rmw-xrce/src/vtable.c`) exists but the
  Zephyr module cmake doesn't compile it.
- C/C++ images are unaffected: `libnros_c.a` is built from `nros-c` whose
  `rmw-zenoh = ["rmw-cffi", "dep:nros-rmw-zenoh"]` features are REAL, so the
  register symbols ride in.

Verified: `nm librustapp.a` in `build-rs-listener-zenoh` and
`build-rs-listener-xrce` has zero occurrences of the register symbols.

## History / how it surfaced

Historically the Rust examples' `nros` dep carried backend features, so the
lane worked (the issue-#35-era zephyr rust zenoh e2e was green). Phase-248/249
moved registration to "board/platform-owned" and the app features went inert —
from that point the pure-Rust zenoh/xrce images had no backend and
`Executor::open` failed. Pre-#155 that failure was SILENT (`Err(_) => return`),
so the lane looked like a flake/timeout; #155 made it panic loudly. #155's
strong `nros_app_register_backends` stub for RUST-API images then turned it
into a **link error** (`undefined reference to nros_rmw_zenoh_register`),
which broke the whole `just zephyr build-fixtures` sweep at the first
`rs-*-zenoh` row — that's how #161's full rebuild found it.

Interim fix (landed with #161): the RUST-API stub declares the register entry
`__attribute__((weak))` and calls it only if non-NULL — images LINK again, and
a backend-less image still fails loudly at `Executor::open` (0155 behavior).

## Decision needed (pick one)

1. **Un-inert the markers for the Rust shape** (parity with `nros-c`): app
   `rmw-zenoh = ["dep:nros-rmw-zenoh"]` (+ platform-zephyr forwarding), same
   for xrce. Requires `nros ws sync` template updates (the `.cargo/config.toml`
   patch tables are nros-managed and need rows for the backend crate closure)
   across the 6 example apps + the workspace zephyr entry. This mirrors what
   `libnros_c.a` already contains for C images, so symbol coexistence with the
   module-compiled `zpico.c` is a solved shape.
2. **Module-side registration TUs**: compile `nros-rmw-xrce/src/vtable.c` in
   `nros_rmw_xrce.cmake` (fixes xrce cheaply — C vtable over the C client lib);
   zenoh has no C vtable, so this path alone cannot fix zenoh.
3. **De-scope the cells**: declare pure-Rust Zephyr = cyclonedds-only (+ entry
   images, which use the west-lane umbrella staticlib and are unaffected),
   drop the `rs-*-{zenoh,xrce}` fixture rows and their tests ("no silent
   caps" — an explicit de-scope, not a silent one).

Option 1 restores the historical matrix; option 3 is honest if nobody needs
the lane. Check `tests/zephyr.rs`'s rust-zenoh service e2e
(`get_zephyr_service_{server,client}_native_sim`) before de-scoping.

## References

`zephyr/CMakeLists.txt` (RUST-API weak stub + comment), `zephyr/cmake/`
(`nros_rmw_zenoh.cmake`, `nros_rmw_xrce.cmake`),
`packages/zpico/nros-rmw-zenoh/src/lib.rs` (`cffi_register`),
`packages/xrce/nros-rmw-xrce/src/vtable.c`, archived issues 0155/0161.
