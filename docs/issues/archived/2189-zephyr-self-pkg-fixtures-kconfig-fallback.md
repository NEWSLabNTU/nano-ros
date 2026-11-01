---
id: 2189
title: "The `zephyr_self_pkg` west fixtures configure a C-API image on 3.7 and abort on 4.4"
status: resolved
resolved_in: "2026-11-01"
type: bug
area: [zephyr, testing]
severity: medium
found: 2026-11-01
related: [2187, 2192, 0626]
---

## What was measured

`zephyr_self_pkg_rust` and `zephyr_self_pkg_sibling` are configure-only
(`west-configure`) fixtures under `packages/testing/nros-tests/fixtures/zephyr_self_pkg/`.
Their `prj.conf` holds `CONFIG_NROS=y` and nothing else. The image's
`system.toml` renders `CONFIG_NROS_RMW_ZENOH=y`, `CONFIG_NROS_RUST_API=y` and
the domain into `nros/zephyr.conf`.

On **Zephyr 4.4** both abort at Kconfig, so neither leaves a `.compile-ok`, and
`zephyr_self_pkg_resolve_bringup_handles_relative_path` and
`zephyr_self_pkg_rust_builds_via_shim` fail as MISSING. There are two warnings:

```
warning: The choice symbol NROS_RUST_API ... was selected (set =y), but NROS_C_API ... ended up as the choice selection.
warning: POSIX_PRIORITY_SCHEDULING (defined at lib/posix/options/Kconfig.sched:7) has direct dependencies POSIX_SYSTEM_INTERFACES with value n, but is currently being y-selected by the following symbols:
 - NROS_ZENOH_MULTI_THREAD ...
error: Aborting due to Kconfig warnings
```

On **Zephyr 3.7** the same configure succeeds, and the first warning is still
printed. So `zephyr_self_pkg_rust` configures a **C-API** image on 3.7 as
well. Its name says Rust, and 3.7 never refused it.

## Cause

1. `NROS_RUST_API` `depends on RUST`, and nothing in these fixtures enables
   `CONFIG_RUST`. The example Rust leaves set it in their `prj.conf`, and these
   fixtures do not.
2. On 4.4, `POSIX_PRIORITY_SCHEDULING` depends on `POSIX_SYSTEM_INTERFACES`.
   3.7 has no such dependency. `NROS_ZENOH_MULTI_THREAD` selects it, and these
   fixtures set no `CONFIG_POSIX_API`, which the example confs set and which is
   what turns `POSIX_SYSTEM_INTERFACES` on.

The second one is in our own `zephyr/Kconfig`, so it can hit any minimal zenoh
image on 4.4, not just these two.

## Resolution

The fixtures prove the configure-time bake of `nros_system_generate`, so
neither was edited. Both causes are fixed where every image meets them.

- **The API fallback, in the renderer.** `leaf_kconfig::render` states
  `CONFIG_RUST=y` beside `CONFIG_NROS_RUST_API=y` when the entry is Rust. The
  entry being Rust is what requires Zephyr's Rust support, and the rendered
  fragment merges last, so a leaf conf can no longer leave it out and get a
  C-API image without being told.
- **The POSIX select, in `zephyr/Kconfig`.** `NROS_ZENOH_MULTI_THREAD` now
  selects `POSIX_PRIORITY_SCHEDULING if POSIX_THREADS`. The scheduling calls
  run only for a zenoh task, which is a pthread, so `POSIX_THREADS` is the
  real precondition. It is defined on both lines, and on 4.4 it implies
  `POSIX_SYSTEM_INTERFACES`. An image without pthreads cannot compile
  zenoh-pico's Zephyr port (`<pthread.h>`), so it now fails at compile time,
  loudly, instead of at Kconfig.
- **The renderer also states `CONFIG_POSIX_API=y` for `rmw = "zenoh"`.**
  That is the system.toml road's equivalent of the `nros-zenoh` snippet's
  line. A `select POSIX_API` on `NROS_RMW_ZENOH` was tried first, and 3.7
  refuses it as a dependency loop: `POSIX_API` selects `POSIX_NETWORKING`,
  and the RMW choice's `NROS_RMW_XRCE` depends on `NET_SOCKETS`.

None of this changes an image that already built. In the issue-2187 sweep's
build dirs, every zenoh image had `POSIX_API=y` (13 on 3.7, 47 on 4.4), and
every multi-thread image had `POSIX_THREADS=y` and
`POSIX_PRIORITY_SCHEDULING=y`.

## Measured

- Both rows configure on 4.4 and on 3.7, with no fallback or select warning.
  `zephyr_self_pkg_rust` now has `CONFIG_RUST=y` and `CONFIG_NROS_RUST_API=y`
  on both lines; on 3.7 it used to fall back to `NROS_C_API`.
- `nros-tests::zephyr_self_pkg`: 2/2 on 3.7 and 2/2 on 4.4. Each line was
  rebuilt immediately before its run, because the two lines share a
  west-fixture build dir (issue 2192).
- `leaf_kconfig` unit tests pass, including the new
  `a_rust_entry_states_the_rust_support_its_api_depends_on`.

## Not done

- The sibling row's `caller/` application gets no rendered fragment at all.
  Its image lives in `alpha_pkg/system.toml` as `entry = "caller"`, and the
  module renders only the application's own `system.toml`, so `caller`
  configures on Kconfig defaults (C API, no NSOS snippet). Its test asserts
  the bake, which lands. Whether a sibling image's statements should reach
  the entry's configure is not decided here.
- Issue 2192, found here: west fixtures are reused across Zephyr lines.
