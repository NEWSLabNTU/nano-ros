
## zenoh build blocks (phase-290)

Each platform directory's `nros-platform.toml` carries the `[build.zenoh]`
block relocated verbatim from the retired central
`packages/zpico/zpico-sys/zenoh_platforms.toml` (phase-136.4 → phase-290).
Original file header preserved below for the schema commentary:

```
# Phase 136.1 / 136.4 — canonical zenoh-pico build manifest.
#
# build.rs reads this once. Cargo features select the platform.
# Per-platform blocks declare every datum the unified
# `build_zenoh_pico(plat: &ResolvedPlatform, target: &str, …)`
# consumer needs:
#   - `defines` / `defines_kv` / `defines_env` — preprocessor flags
#   - `include` — glob roots under `zenoh-pico/src/` for core sources
#   - `extra_sources` — additional .c files outside `zenoh-pico/src/`
#     (interpolated via `{nros}` / `{src}` / `{out}` / `{env:VAR}`)
#   - `required_env` — SDK paths the build needs, with help text
#     and optional sub-dir validation
#   - `include_paths` / `include_paths_conditional` — header search
#     paths (interpolated; gated by `target_match` / `target_not` /
#     `if_env`)
#   - `arch` — name of `[arch.*]` block to apply for target-arch
#     compiler flags
#   - `compile` — opt_level / warnings / extra cflags
#   - `pic` — `cc::Build::pic` override (NuttX flat builds)
#   - `link.*` — per-link-feature policy mask (Phase 134)
#
# Adding a new platform = one new `[platform.<name>]` block; no
# `build.rs` edits.

# ----------------------------------------------------------------
# Arch profiles — reusable target-arch compiler flag tables.
# ----------------------------------------------------------------

```
