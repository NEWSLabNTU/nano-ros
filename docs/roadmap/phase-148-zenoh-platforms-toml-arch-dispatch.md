# Phase 148 — zenoh_platforms.toml Per-Target Arch Dispatch

**Goal.** Fix `zpico-sys` manifest's platform→arch mapping so
ESP32-C3 (riscv32imc bare-metal) gets the `riscv32imc` arch profile
applied, not `cortex-m3`. Today `bare-metal` platform hard-codes
`arch = "cortex-m3"`, so `arch_matches` fails for riscv32imc target,
`apply_arch` is skipped, picolibc sysroot is never added to the cc-rs
include list, and `cargo check` on every ESP32 example fails with
`stdint.h: No such file or directory`.

**Status.** Not started.

**Priority.** P2 — blocks `just check` on ESP32 examples but doesn't
regress production builds (ESP32 examples weren't on the green path
before either; they pre-date the Phase 136 manifest refactor).

**Depends on.** None. Self-contained Phase 136 follow-up.

**Related.** Phase 136 (manifest-driven cc-rs collapse — introduced
the platform→arch mapping that this phase fixes), Phase 140 (post-140
CI surfaced the failure during `just check` fan-out), Phase 146
(other zenoh-pico embedded link regressions).

---

## Symptom

```
$ cd examples/esp32/rust/zenoh/listener && cargo check
warning: zpico-sys@0.1.0: In file included from
  .../zenoh-pico/src/api/admin_space.c:15:
warning: zpico-sys@0.1.0:
  /usr/lib/gcc/riscv64-unknown-elf/10.2.0/include/stdint.h:9:16:
  fatal error: stdint.h: No such file or directory
```

`riscv64-unknown-elf-gcc`'s stub `stdint.h` does
`#include_next <stdint.h>` expecting picolibc's `stdint.h` next on
the search path. The picolibc include
(`/usr/lib/picolibc/riscv64-unknown-elf/include/`) is NOT on the
cc-rs `-I` list because `apply_arch(riscv32imc, …)` never ran.

## Root cause

`packages/zpico/zpico-sys/zenoh_platforms.toml`:

```toml
[platform.bare-metal]
arch = "cortex-m3"      # ← hard-coded; wrong for riscv32imc targets
```

`packages/zpico/zpico-sys/build.rs::build_zenoh_pico_unified`:

```rust
if let Some(arch_name) = plat.arch.as_deref() {
    if let Some(arch) = arch_table.get(arch_name) {
        if arch_matches(arch, target) {     // ← false for riscv32imc vs cortex-m3
            apply_arch(arch, &mut build, out_dir);   // ← never called
        }
    }
}
```

`apply_arch` is where `needs_picolibc=true` triggers the
`get_picolibc_sysroot()` lookup that would add the missing include.

## Fix options

### A. Per-target platform entries

Split `[platform.bare-metal]` into `[platform.bare-metal-cortex-m3]`
and `[platform.bare-metal-riscv32imc]`. Each has the right `arch`.
build.rs picks the platform name based on target triple:

```rust
} else if use_bare_metal {
    if target.contains("thumbv") {
        Some("bare-metal-cortex-m3")
    } else if target.contains("riscv32") {
        Some("bare-metal-riscv32imc")
    } else {
        panic!("bare-metal target unrecognised: {target}")
    }
}
```

Pro: explicit. Con: combinatorial explosion if more bare-metal arches
land.

### B. Manifest `arch` becomes an array; build.rs picks first matching

```toml
[platform.bare-metal]
arch = ["cortex-m3", "riscv32imc"]   # first arch_matches wins
```

`build.rs` iterates and applies the first arch whose `target_match`
hits. `arch_matches` already does the matching; just call it on
each entry.

Pro: scales without per-target platform entries. Con: order
matters; less explicit per-target.

### C. Auto-derive arch from target

Drop `arch` from platform entries entirely. `build.rs` iterates
`arch_table` and applies whichever entry's `target_match` hits
the build target. Platform only declares non-arch deltas.

Pro: cleanest; manifest stops conflating platform with arch. Con:
larger refactor; some platforms may need arch-specific defines
that don't fit the per-arch model.

Recommend **B** — smallest manifest change, scales for future
RTOS-on-RISC-V combinations, keeps the platform/arch split that
Phase 136 introduced.

---

## Work Items

- [ ] **148.1 — Manifest schema bump.**
      `[platform.bare-metal].arch` becomes `array` instead of
      `scalar`. Other platform entries can stay scalar
      (`arch = "cortex-m3"` is treated as a single-element array
      by deserialiser).
      **Files.** `packages/zpico/zpico-sys/build/manifest.rs`,
      `packages/zpico/zpico-sys/zenoh_platforms.toml`.

- [ ] **148.2 — `bare-metal` platform manifest update.**
      ```toml
      [platform.bare-metal]
      arch = ["cortex-m3", "riscv32imc"]
      ```
      Plus any platform that supports multiple arches (verify
      against current manifest).
      **Files.** `packages/zpico/zpico-sys/zenoh_platforms.toml`.

- [ ] **148.3 — `build.rs` first-match dispatch.**
      ```rust
      for arch_name in &plat.arch {
          if let Some(arch) = arch_table.get(arch_name.as_str())
              && arch_matches(arch, target)
          {
              apply_arch(arch, &mut build, out_dir);
              break;
          }
      }
      ```
      **Files.** `packages/zpico/zpico-sys/build.rs`.

- [ ] **148.4 — Per-target smoke verification.**
      ```bash
      cd examples/esp32/rust/zenoh/listener && cargo check
      cd examples/qemu-arm-baremetal/rust/zenoh/talker && cargo check
      ```
      Both must succeed.
      **Files.** none (verification).

- [ ] **148.5 — Doc + CLAUDE.md note.**
      Document the platform→arch array contract in
      `book/src/internals/zpico-build.md` (Phase 136 page).
      Mirrors `CLAUDE.md`'s build-tier note that platforms can
      carry multiple architectures.
      **Files.** `book/src/internals/zpico-build.md`, `CLAUDE.md`.

---

## Acceptance

- [ ] `cargo check` succeeds on `examples/esp32/rust/zenoh/{listener,talker}`.
- [ ] `cargo check` on `examples/qemu-arm-baremetal/rust/zenoh/talker`
      still succeeds (cortex-m3 path unbroken).
- [ ] `just check` passes the per-example fan-out (no
      `stdint.h: No such file or directory` for any bare-metal target).
- [ ] `cargo metadata --no-deps` clean.

---

## Notes

- **Why P2.** ESP32 examples weren't in the green-path coverage
  before Phase 136 either; they predated the manifest. The failure
  is loud (cargo errors with the actual stdint message) so users
  diagnosing ESP32 builds find it quickly. Not a silent corruption
  class; safe to defer.
- **Why not option C (auto-derive arch from target).** Tempting but
  larger refactor — some platforms (zephyr, orin-spe) carry
  arch-specific defines that the current platform-scoped
  `defines_kv` may not handle cleanly when arch dispatch becomes
  platform-orthogonal. Option B is the minimum change to fix the
  bug without committing to a deeper rearchitecture.
- **Phase 146 covers different ground.** Phase 146's three link
  regressions are LINKER failures (duplicate `_z_task_free`,
  missing `_z_*_serial_internal`). Phase 148 is a COMPILER failure
  (missing `stdint.h`) one step earlier in the chain. Both surface
  because cargo check exercises embedded paths the legacy install
  path didn't; both are Phase 136 manifest-refactor follow-throughs.
