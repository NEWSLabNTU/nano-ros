# Vendor-lib deploy template (Phase 172.V)

A copy-out `deploy/<name>/` shell for the **vendor-lib** ownership model:
nano-ros owns the toolchain and emits the generated wiring as a **compiled
entry library** (`lib<sys>.a` + `<sys>.h`); the vendor's final binary links
that staticlib + this startup object against the vendor SDK. Canonical target:
**NVIDIA Orin SPE** (links `libtegra_aon_fsp.a`).

This is one of the per-platform templates from Phase 172 WP-C. It exercises the
WP-B compiled-form entry lib + the WP-A `nros deploy` command-runner — no
per-vendor code lives in nano-ros; the vendor knowledge is the `build[]` /
`package[]` shell lines you author in the root `nros.toml`.

## Use

1. Copy this dir to your workspace as `deploy/<name>/`.
2. Add a `[deploy.<name>]` table to the root `nros.toml` (see below).
3. Edit `startup.c`: replace `mysys` with your `[system]` name (lowercased,
   non-alphanumeric → `_`) and `#include "<sys>.h"`.
4. `nros deploy <name>` — emits `lib<sys>.a` + `<sys>.h`, substitutes the
   var-set into `build[]` / `package[]`, and runs them.

## `[deploy.<name>]`

```toml
[deploy.orin]
kind   = "vendor-lib"
target = "armv7r-none-eabihf"
self   = "deploy/orin"
emit   = "compiled"                       # nano-ros owns the toolchain
vendor.dir = { env = "NV_SPE_FSP_DIR" }   # vendor SDK root
vendor.pin = "spe-fsp 36.3"               # asserted before the build (drift guard)
build = [
  "arm-none-eabi-gcc {self}/startup.c {entry_lib} -I{self} \
   -L{vendor.dir}/lib -ltegra_aon_fsp -T {self}/spe.ld -o build/orin/spe.elf",
]
package = ["python3 {vendor.dir}/tools/spe_sign.py build/orin/spe.elf -o build/orin/spe.bin"]
```

## Runner var-set

`nros deploy` substitutes these into every `build[]` / `package[]` step:

| var | value |
|---|---|
| `{self}` | absolute `deploy/<name>/` |
| `{entry_lib}` | the compiled `lib<sys>.a` |
| `{entry_header}` | the generated `<sys>.h` |
| `{entry_src}` | the generated crate dir (source form) |
| `{board}` | `[deploy].board` |
| `{target}` | `[deploy].target` |
| `{vendor.dir}` | resolved vendor SDK root |

## C ABI

`startup.c` drives the `nros_<sys>_*` C ABI from `<sys>.h`:

- `NrosExecutor *nros_<sys>_build_executor(const NrosConfig *cfg)` — open the
  session; `cfg = NULL` ⇒ env/baked config (precedence param > env > baked).
- `int32_t nros_<sys>_register_all(NrosExecutor *)` — register sched contexts,
  every node, lifecycle, parameter persistence.
- `int32_t nros_<sys>_spin(NrosExecutor *)` — blocking spin until shutdown.
- `void nros_<sys>_destroy(NrosExecutor *)` — free the executor.
