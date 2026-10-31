---
id: 2189
title: "The `zephyr_self_pkg` west fixtures configure a C-API image on 3.7 and abort on 4.4"
status: open
type: bug
area: [zephyr, testing]
severity: medium
found: 2026-11-01
related: [2187]
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

## Cause (read, not yet confirmed by a build)

1. `NROS_RUST_API` `depends on RUST`, and nothing in these fixtures enables
   `CONFIG_RUST`. The example Rust leaves set it in their `prj.conf`, and these
   fixtures do not.
2. On 4.4, `POSIX_PRIORITY_SCHEDULING` depends on `POSIX_SYSTEM_INTERFACES`.
   3.7 has no such dependency. `NROS_ZENOH_MULTI_THREAD` selects it, and these
   fixtures set no `CONFIG_POSIX_API`, which the example confs set and which is
   what turns `POSIX_SYSTEM_INTERFACES` on.

The second one is in our own `zephyr/Kconfig`, so it can hit any minimal zenoh
image on 4.4, not just these two.

## Not done

Not fixed in issue 2187's change. The two fixtures need a decision on what they
are meant to prove. And the `select` in `zephyr/Kconfig` has to work on both
lines: `POSIX_SYSTEM_INTERFACES` has to be checked on 3.7 before anything
selects it there.
