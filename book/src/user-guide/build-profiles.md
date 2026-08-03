# Build Profiles

nano-ros builds both C/C++ and Rust code on your behalf. You choose the
optimization level **once, in your own build system**, and nano-ros propagates
it to every crate and library it builds for you.

There are two knobs, not one, because the two languages have different option
vocabularies and collapsing them would mean inventing a lossy middle language:

| You are building with | You set | nano-ros propagates it to |
| --- | --- | --- |
| CMake | `CMAKE_BUILD_TYPE` | your C/C++ code, and (mapped) the Rust it builds |
| Cargo only | a cargo profile | every nano-ros crate, as a normal path dependency |

## From a CMake build type

Set `CMAKE_BUILD_TYPE` as you would in any CMake project. nano-ros derives the
cargo profile from it:

| `CMAKE_BUILD_TYPE` | cargo profile | what it means |
| --- | --- | --- |
| `Debug` | `dev` | debuggable, minimal optimization |
| `RelWithDebInfo` | `nros-relwithdebinfo` | opt-level 2, debug info, no LTO — **the default** |
| `MinSizeRel` | `nros-minsizerel` | opt-level `"s"`, fat LTO — smallest images |
| `Release` | `release` | opt-level 3, fat LTO — fastest code |
| *(unset)* | `nros-relwithdebinfo` | the development default |

```bash
cmake -B build -DCMAKE_BUILD_TYPE=MinSizeRel
cmake --build build
```

Your C/C++ sources get `-Os` from CMake as usual, and the Rust nano-ros builds
for you gets `--profile nros-minsizerel`. **You do not need to add anything to
any `Cargo.toml`** — nano-ros supplies the definition of its own `nros-*`
profiles.

An unrecognized build type is an error rather than a guess, so a custom type
never silently produces an optimization level you did not ask for.

## Choosing the Rust profile separately

`NROS_CARGO_PROFILE` overrides the mapping when you want the two halves to
differ — for example a small C/C++ image with debuggable Rust:

```bash
cmake -B build -DCMAKE_BUILD_TYPE=MinSizeRel -DNROS_CARGO_PROFILE=nros-relwithdebinfo
```

## Using your own profile

Name any profile you like. When the name is not one of nano-ros's `nros-*`
profiles, **you own the definition** — nano-ros passes the name through and
injects nothing, so your settings are authoritative:

```toml
# your-workspace/Cargo.toml
[profile.prod]
inherits = "release"
opt-level = 3
lto = "fat"
```

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Release -DNROS_CARGO_PROFILE=prod
```

If the profile is not defined, cargo reports
`error: profile 'prod' is not defined` and points at your manifest — which is
the right file to fix.

## A pure-Cargo workspace

Nothing to configure. nano-ros crates are ordinary path dependencies, so your
workspace-root profile already governs them:

```bash
cargo build --profile prod
```

## Which profile is active?

```bash
nros profile resolve --build-type MinSizeRel   # -> nros-minsizerel
nros profile dir     nros-minsizerel           # -> the target/ subdirectory
```

A CMake configure also prints it:

```text
-- nano-ros: cargo profile `nros-minsizerel` (CMAKE_BUILD_TYPE=MinSizeRel) → target/nros-minsizerel
```

## Notes for embedded targets

- **Images grow at the default.** `nros-relwithdebinfo` trades size for build
  speed. For flashing to a constrained board, build `MinSizeRel`.
- **Two platforms pin their own profile** regardless of what you choose, and
  will say so: NuttX Rust images (a codegen miscompile at `lto = "off"`) and
  FreeRTOS QEMU images (the emulated Cortex-M3 misses zenoh-pico's handshake
  window at low optimization).
