---
id: 432
title: "zephyr-lang-rust's DT codegen emits a 5-arg `GpioPin::new` against a 6-arg signature, so Rust cannot build for any Zephyr board with gpio nodes"
status: open
type: bug
area: platform-zephyr
related: [phase-337, rfc-0064]
---

## Symptom

Building any Rust nano-ros example for a real Zephyr board fails inside the
`zephyr` crate itself — before a single line of nano-ros code is compiled:

```
error[E0061]: this function takes 6 arguments but 5 arguments were supplied
help: provide the argument
    |
448 |   crate::device::gpio::GpioPin::new(&UNIQUE, &STATIC, device, device_static, 1u32, /* u32 */)
    |                                                                                   +++++++++++
```

Four occurrences on `mps2_an385`. `GpioPin::new` (`zephyr/src/device/gpio.rs`)
takes `(unique, _static, device, device_static, pin: u32, dt_flags: u32)`; the
devicetree generator emits only the `pin` cell.

Pinned module: `zephyr-lang-rust` @ `404fcef` ("Add Clippy to CI (#157)"),
`zephyr-workspace/modules/lang/rust`.

## Why it has never been seen

`native_sim/native/64` is the only Zephyr target this repo has ever built, and
it has no gpio nodes, so the augment never matches and the calls are never
generated. phase-337 W2.b added the first non-native_sim Zephyr board and hit
it immediately.

## `CONFIG_GPIO=n` does not dodge it — it makes it worse

The generator reads the DEVICETREE, not Kconfig. Two of the three gpio augments
in `dt-rust.yaml` carry `cfg: CONFIG_GPIO`, but `gpio-keys` (line 35) carries
**no `cfg:` key at all**, so its `GpioPin` instances are still emitted while the
`raw` bindings they reference disappear with the driver:

```
error[E0425]: cannot find type `gpio_dt_spec` in module `raw`
error[E0425]: cannot find function `gpio_pin_configure` in module `raw`
...
```

14 errors instead of 4. Both halves are upstream bugs: the missing `dt_flags`
argument, and the missing `cfg:` on the `gpio-keys` augment.

## Impact

Rust-on-Zephyr is native_sim-only, and that is a hard block, not a slowdown:
no board with gpio nodes in its DTS can build the `zephyr` crate at all. Since
essentially every real board has gpio, this is "Rust on Zephyr hardware does not
build" until fixed.

The C and C++ entries are unaffected — they do not involve the `zephyr` crate.
phase-337 W2.b's cells therefore build `examples/zephyr/c/*`, which exercises
everything the Cortex-M witness exists for (32-bit pointers, Zephyr's in-kernel
IP stack, a real ethernet driver) without touching this path.

## Fix shape

Upstream, in `zephyr-build`'s devicetree generator: emit the second phandle cell
for `!Phandle gpios` instances, and add the missing `cfg: CONFIG_GPIO` to the
`gpio-keys` augment. Both are small.

Downstream, the repo already has the delivery mechanism — `zephyr/patches.yml`
plus `scripts/zephyr/*.sh` (see the four NSOS/pthread patches). Note the
existing entries are all `module: zephyr`; this would be the first patch against
the `zephyr-lang-rust` module, so the in-tree script path needs to learn that
module too.

Until then: a Rust image for a Zephyr board is only possible on a board whose
devicetree has no gpio nodes.

## Related

- phase-337 W2.b — the witness that found it; its board conf carries the same
  explanation at the point of use.
- RFC-0064 — a board is a config, not a crate; this is the counter-example where
  the config is fine and a vendored toolchain is not.
