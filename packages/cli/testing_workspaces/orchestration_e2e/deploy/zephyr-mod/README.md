# `zephyr-mod` — Zephyr vendor-module deploy glue (Phase 172 W.4)

The `[deploy.zephyr-mod]` target's `self` dir. nano-ros emits the *source* form
of the system wiring at `{entry_src}` — for Zephyr that generated crate IS a
west-buildable app (`rustapp` staticlib + `rust_cargo_application()` CMake +
prj.conf). The vendor (`west`) owns the build:

```
nros deploy zephyr-mod
  → generate the Zephyr entry app at {entry_src}
  → west build -b native_sim/native/64 -d build/zephyr-mod {entry_src}
```

This dir holds the vendor glue that isn't part of the generated app:

- `west.yml` — the manifest import that makes the nano-ros Zephyr module
  (`integrations/zephyr/`) discoverable to `west` (so `NanoRos::NanoRos` links).
- (optional) Kconfig / prj overlays referenced via a `[deploy.zephyr-mod.config]`
  hook, merged with `EXTRA_CONF_FILE={self}/<overlay>.conf`.

The real `west` cross-build + native_sim boot is the W.4 step 2/3 follow-up; the
deploy *wiring* (resolve + var-set substitution) is validated host-side by the
`deploy_zephyr_vendor_module_dry_run_resolves_and_substitutes` e2e.
