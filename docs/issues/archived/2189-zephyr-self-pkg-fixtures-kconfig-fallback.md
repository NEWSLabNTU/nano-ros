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

## The sibling road (second change)

The first change left one gap. A hand-written application whose image lives
in a sibling bringup (`entry = "caller"` in `alpha_pkg/system.toml`) got none
of that image in Kconfig. The module's Kconfig hook runs before the
application's `CMakeLists.txt`, so it cannot read `nros_system_generate(alpha_pkg)`,
and it renders only the application's own `system.toml` or an explicit
`-DNROS_IMAGE_SYSTEM_DIR` + `-DNROS_IMAGE`. The later bake does find the image
through `entry =`, so the two halves of one configure read different
configurations without a word. Measured on three rows: the self-pkg sibling
(C API, no NSOS snippet), `west_bringup_zephyr`, and
`west_bringup_zephyr_cyclone_user_config`. The last one built Cyclone while
the only image claiming its application said `rmw = "zenoh"`.

Three options were weighed with the maintainer (2026-11-01): refuse and
name the fix; have the hook search for the image that claims the application;
or retire the sibling form. **Refuse and name the fix was chosen.** It adds no
search rule (the hook would have to guess a scope for bringups, and a wrong
guess applies another image's knobs silently) and keeps the issue-1312 shape.

- The hook records the image it rendered (`NROS_IMAGE_RENDERED_DIR` /
  `_IMAGE`, global properties), and `codegen-system` prints the image it
  resolved as `NROS_SYSTEM_IMAGE=<id>`.
- `nros_system_generate()` refuses a bake whose image is not the one Kconfig
  rendered, and prints the two arguments that make the hook render it. When
  the configure was told its image, the bake is that image (`--target`), and a
  bringup argument naming a different directory is refused.
- `west_extra` is now split into arguments on whitespace (as the manifest's
  RMW reader already read it), so a row can carry the pointer. The three rows
  carry it, and the cyclone row has its own `[image.zephyr_native_sim_cyclonedds]`
  with no `entry`, selected by `-DNROS_IMAGE`.

Measured: all four west fixtures build on 3.7 and 4.4 with their image
rendered, and the sibling row without the pointer is refused with the message
naming `-DNROS_IMAGE_SYSTEM_DIR=../alpha_pkg -DNROS_IMAGE=zephyr`.
