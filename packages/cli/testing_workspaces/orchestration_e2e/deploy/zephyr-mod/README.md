# `zephyr-mod` — Zephyr vendor-module deploy glue (Phase 172 W.4)

The `[deploy.zephyr-mod]` target's `self` dir. nano-ros emits the *source* form
of the system wiring at `{entry_src}` — for Zephyr that generated crate IS a
west-buildable app (`rustapp` staticlib + `rust_cargo_application()` CMake +
prj.conf). The vendor (`west`) owns the build:

The platform build shape is:

```sh
west build -b native_sim/native/64 -d build/zephyr-mod <entry_src>
```

This dir holds the vendor glue that isn't part of the generated app:

- `west.yml` — the manifest import that makes the nano-ros Zephyr module
  (`integrations/zephyr/`) discoverable to `west` (so `NanoRos::NanoRos` links).
- (optional) Kconfig / prj overlays referenced via a `[deploy.zephyr-mod.config]`
  hook, merged with `EXTRA_CONF_FILE={self}/<overlay>.conf`.

The real `west` cross-build + native_sim boot is the W.4 step 2/3 follow-up.
