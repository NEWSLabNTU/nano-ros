# Build Profiles

nano-ros builds both C/C++ and Rust code on your behalf. You choose the
optimization level **once**, and nano-ros propagates it to every crate and
library it builds for you.

The image is where you say it, beside the board it is built for:

```toml
[image_defaults]
profile = "nros-relwithdebinfo"   # the base every image folds over

[image.native]
board = "native"

[image.mps2]
board   = "qemu-mps2-an385"
profile = "nros-minsizerel"       # this one is going on a board
```

`nros build` carries that through to whichever tool builds the image — the
cargo profile flag on the cargo driver, `CMAKE_BUILD_TYPE` on the cmake one.
You do not repeat it on the command line, and you do not define the `nros-*`
profiles anywhere: `nros sync` writes them into the image's generated cargo
settings file under `build/` (`build/native/nros-cargo.toml` for a
single-package project), along with everything else the board choice
implies.

## From a CMake build type

When you drive CMake yourself — a single-package C or C++ project builds
with its own `cmake -B build`, and needs no sync — set `CMAKE_BUILD_TYPE`
as you would in any CMake project. nano-ros derives the cargo profile for
the Rust it builds underneath from it:

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
[profile.prod]
inherits = "release"
opt-level = 3
lto = "fat"
```

Put that where cargo already looks for a profile: the package's own
`Cargo.toml` for a single-package project, or a `.cargo/config.toml` in a
directory above your project. There is no workspace-root `Cargo.toml` to
put it in — a nano-ros workspace has no root build file, and the cargo root
is the generated entry under `build/`, which is regenerated on every build.

A config file is a first-class home for a profile, not a workaround: it is
exactly how the `nros-*` presets reach cargo. An undefined name still fails
the way it always did — `error: profile 'prod' is not defined` — while a
name the generated settings file defines resolves and builds.

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Release -DNROS_CARGO_PROFILE=prod
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
