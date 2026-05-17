# Zephyr (integration shell)

> **Contributor docs?** Building nano-ros's own Zephyr examples from
> this repository is covered at [Zephyr (contributor)](./zephyr.md).

Phase 139 ships a thin Zephyr module under `integrations/zephyr/`
that lets `west` discover nano-ros automatically. Use this path
when consuming nano-ros from a Zephyr workspace alongside other
modules; it replaces hand-rolled `add_subdirectory(nano-ros)` glue
in `prj.conf` projects.

## Prereqs

- Zephyr SDK ≥ v0.16
- `west` CLI (`pip install west`)

## One-liner add to your workspace `west.yml`

```yaml
manifest:
  remotes:
    - name: nano-ros
      url-base: https://github.com/aeon/nano-ros
  projects:
    - name: nano-ros
      remote: nano-ros
      path: modules/nano-ros
      import:
        file: integrations/zephyr/west.yml
```

Then:

```bash
west update
west build -b native_sim/native/64 path/to/your/app
```

## Minimal user `prj.conf`

```
CONFIG_NROS=y
CONFIG_NROS_RMW="zenoh"
CONFIG_NROS_ROS_EDITION="humble"
```

## Minimal user `main.c`

```c
#include <nros/init.h>

int main(void) {
    nros_support_t support = nros_support_get_zero_initialized();
    (void)support;
    return 0;
}
```

`CONFIG_NROS=y` activates the shell; the shell maps Kconfig values
to `NANO_ROS_*` CMake cache vars and `add_subdirectory()`s the root
nano-ros CMake (Phase 137). `NanoRos::NanoRos` is linked into the
`app` library transparently.

## Coexistence with the legacy `zephyr/` module

The pre-Phase 139 module at `<repo-root>/zephyr/` (vendored
zenoh-pico transport sources) still works. The Phase 139 shell is
INDEPENDENT — pick one entry point per workspace. Phase 140 removes
the legacy path.
