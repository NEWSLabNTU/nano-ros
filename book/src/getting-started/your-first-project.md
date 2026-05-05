# Your first nano-ros project

Five commands from a clean clone to a running talker. This page assumes
you have rust + a C compiler installed; everything else `nros doctor`
flags for you.

## 1. Build the CLI

```sh
cd packages/codegen/packages
cargo install --path nros-cli
```

`nros --version` should now resolve.

## 2. Diagnose

```sh
nros doctor
```

Calls every per-module doctor recipe (`nuttx`, `zephyr`, `freertos`,
…). Returns 0 when everything you need is in place; non-zero with a
fixit hint otherwise.

## 3. Scaffold

```sh
nros new my_talker --platform posix --lang rust
cd my_talker
```

Generates a `package.xml`, `Cargo.toml`, `src/main.rs`, and (for
embedded platforms) a `config.toml`. Swap `--platform posix` for any
of `native | freertos | nuttx | threadx | zephyr | esp32 | baremetal`.

## 4. Build

```sh
nros build
```

`nros` auto-detects `Cargo.toml` here and runs `cargo build`. For C/C++
projects it picks cmake; for Zephyr (`prj.conf`) it picks `west build`.

## 5. Run

```sh
nros run
```

For the POSIX scaffold above, this becomes `cargo run`; you should
see `Hello from my_talker!` printed.

For an ESP32 scaffold (when the `.cargo/config.toml` target is
`xtensa-esp32*` or `riscv32imc*`), `nros run` chains
`espflash flash --monitor` instead.

---

Next up: pick a real RTOS in
[Getting started → FreeRTOS](./freertos.md), [Zephyr](./zephyr.md),
[NuttX](./nuttx.md), [ThreadX](./threadx.md), or
[ESP32](./esp32.md). The full `nros` verb surface is on the
[CLI reference page](../reference/cli.md).
