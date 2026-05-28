# Vendor-module deploy template (Phase 172 W.4)

The **vendor-module** ownership model: the *vendor* owns the build and toolchain;
nano-ros is a guest **source** module the vendor compiles. nano-ros emits the
generated wiring as a **source** form (a crate + a vendor-includable CMake
fragment), and the vendor's build (`west` / `make` / `idf.py`) compiles it.
Canonical target: **Zephyr** (`west`).

Unlike *vendor-lib* (nano-ros owns the toolchain, ships `lib<sys>.a` + `<sys>.h`)
and *self* (nano-ros owns the whole binary), here nano-ros hands the vendor
source and steps aside. No per-vendor code lives in nano-ros — the vendor
knowledge is the `build[]` / `package[]` shell lines you author in the root
`nros.toml`.

## Zephyr: the generated crate IS the west app

For a Zephyr deploy the source form is *not* the generic Corrosion fragment —
the generator emits a west-buildable Zephyr application directly:

- `src/lib.rs` — the entry wiring as a `rustapp` **staticlib** crate.
- `CMakeLists.txt` — `find_package(Zephyr)` + `rust_cargo_application()`
  (zephyr-lang-rust links the staticlib into the Zephyr image).
- `prj.conf` — `CONFIG_RUST=y` / `CONFIG_RUST_ALLOC=y` / `CONFIG_NROS=y` +
  the net overlay for the chosen RMW.

So the `build[]` step just points `west` at `{entry_src}`. nano-ros is
discovered as a Zephyr module via `integrations/zephyr/` (`module.yml` +
`west.yml`); the consuming workspace imports that west fragment.

(For a generic CMake vendor instead of Zephyr, the source form is the Corrosion
fragment: `add_subdirectory({entry_src} <sys>_entry)` +
`target_link_libraries(<app> PRIVATE <sys>_entry)`.)

## Use

1. Add a `[deploy.<name>]` table to the root `nros.toml` (see below).
2. Ensure the nano-ros Zephyr module is on west's path (import
   `integrations/zephyr/west.yml`), and `ZEPHYR_BASE` is set (`just zephyr setup`).
3. `nros deploy <name>` — generates the Zephyr entry app under `{entry_src}`,
   substitutes the var-set into `build[]` / `package[]`, and runs them.

## `[deploy.<name>]`

```toml
[deploy.zephyr-mod]
kind   = "vendor-module"
board  = "native_sim/native/64"        # any Zephyr board
target = "x86_64-unknown-linux-gnu"     # the triple the board compiles to
self   = "deploy/zephyr-mod"            # required: the module glue dir (west.yml import, overlays)
# emit defaults to "source" for vendor-module.
build = [
  "west build -b {board} -d build/zephyr-mod {entry_src}",
]
package = ["echo zephyr-mod image: build/zephyr-mod/zephyr/zephyr.exe"]
```

`kind = "vendor-module"` requires both a `build = [...]` step and `self =
"deploy/<name>"` (the module glue dir — at minimum a `west.yml` that imports the
nano-ros Zephyr module so `west build {entry_src}` finds `NanoRos::NanoRos`).

An optional `[deploy.<name>.config]` hook injects a generated Kconfig fragment
the vendor build merges (e.g. `EXTRA_CONF_FILE={self}/transport.conf`).

## Runner var-set

`nros deploy` substitutes these into every `build[]` / `package[]` step:

| var | value |
|---|---|
| `{entry_src}` | the generated entry crate dir (the west app) |
| `{self}` | absolute `deploy/<name>/` (the module glue dir — required) |
| `{board}` | `[deploy].board` |
| `{target}` | `[deploy].target` |
| `{entry_header}` / `{entry_lib}` | generated header / staticlib (unused by the source form) |
| `{vendor.dir}` | resolved vendor SDK root (if `vendor` is declared) |
