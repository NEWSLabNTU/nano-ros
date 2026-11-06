# Build profiles

<!-- type: how-to · level: 3 -->

**Needs first:** [Configure an image](configure-an-image.md)

Choose a build profile when an image must be smaller, faster or easier to
debug, and profile the build when it takes too long.

nano-ros builds C, C++ and Rust code on your behalf. You choose the
optimization level once, on the image, and the build passes it to every crate
and library it builds: the cargo profile on the cargo driver,
`CMAKE_BUILD_TYPE` on the CMake one.

## Before you start

- A workspace with an image, or a CMake project that adds nano-ros as in
  [Build inside an existing CMake or vendor tree](existing-cmake-or-vendor-tree.md).

## Steps

### Set the profile on the image

1. Name it in the bringup's `system.toml`, on one image or on
   `[image_defaults]` for all of them:

   ```toml
   [image_defaults]
   profile = "nros-relwithdebinfo"   # the base every image folds over

   [image.linux]
   board = "linux"

   [image.mps2]
   board   = "mps2-an385-baremetal"
   profile = "nros-minsizerel"       # this one goes on a board
   ```

2. Build. You do not repeat the profile on the command line, and you do not
   define the `nros-*` profiles anywhere: the build writes them into the
   image's generated cargo settings under `build/`.

   ```bash
   nros build mps2
   ```

The `nros-*` profiles and the CMake build types they come from:

| `CMAKE_BUILD_TYPE` | cargo profile | Meaning |
| --- | --- | --- |
| `Debug` | `dev` | debuggable, minimal optimization |
| `RelWithDebInfo` | `nros-relwithdebinfo` | opt-level 2, debug info, no LTO; the default |
| `MinSizeRel` | `nros-minsizerel` | opt-level `s`, fat LTO; smallest images |
| `Release` | `release` | opt-level 3, fat LTO; fastest code |
| (unset) | `nros-relwithdebinfo` | the development default |

### Set it from CMake

When you drive CMake yourself, set `CMAKE_BUILD_TYPE` as in any CMake project,
and nano-ros derives the cargo profile for the Rust it builds underneath:

```bash
cmake --preset linux -DCMAKE_BUILD_TYPE=MinSizeRel
cmake --build build/linux
```

The configure prints the profile it chose and the `target/` directory it
builds into:

```text
-- nano-ros: cargo profile `nros-minsizerel` (CMAKE_BUILD_TYPE=MinSizeRel) → target/nros-minsizerel
```

Set `NROS_CARGO_PROFILE` when the two halves should differ, for example a
small C++ image with debuggable Rust:

```bash
cmake --preset linux \
    -DCMAKE_BUILD_TYPE=MinSizeRel -DNROS_CARGO_PROFILE=nros-relwithdebinfo
```

An unrecognized build type is an error, never a guess.

### Use a profile of your own

Name any profile. When the name is not one of the `nros-*` profiles, you own
its definition, and nano-ros passes the name through and injects nothing:

```toml
[profile.prod]
inherits = "release"
opt-level = 3
lto = "fat"
```

Put it where cargo already looks: the package's `Cargo.toml` for a
one-package project, or a `.cargo/config.toml` in a directory above the
project. A nano-ros workspace has no root `Cargo.toml`, and the generated
entry under `build/` is rewritten on every build. An undefined name fails as
cargo always does: `error: profile 'prod' is not defined`.

### Find where the build time goes

`nros-build-profile` reads the timing files the build already wrote and
reports where the time went: code generation, compile and link, which unit
dominated, and which crate was built more than once. It never builds
anything. The release installs it beside `nros`; release `0.6.0-nros4` and
older do not carry it.

1. For per-crate detail on the cargo driver, build with cargo's timings.
   ninja, which the CMake and west drivers use, always writes its log, so
   those need nothing.

   ```bash
   cargo build --timings --manifest-path build/posix-zenoh/linux_entry/Cargo.toml \
       --config build/posix-zenoh/linux_entry/nros-cargo.toml
   ```

2. Point it at the directory that holds `build*/` or `target*/`:

   ```bash
   nros-build-profile build/posix-zenoh/linux_entry --deep
   ```

   It prints a stage table, the slowest units with `--deep`, and hints.
   Without cargo's timings you still get the stage table, and a note on how
   to get the drill-down.

3. For CI, write it as JSON and compare runs:

   ```bash
   nros-build-profile build/posix-zenoh/linux_entry --json
   ```

   The hints flag a large C unit built with no compiler cache, a crate
   compiled more than once, and a job count too high for the RAM. Silence
   them with `--no-hints`.

## Example

One workspace, a debuggable host image and a small board image:

```toml
# src/demo_bringup/system.toml (images)
[image_defaults]
profile = "nros-relwithdebinfo"

[image.linux]
board = "linux"

[image.mps2]
board   = "mps2-an385-baremetal"
profile = "nros-minsizerel"
```

```bash
nros build --all
```

## If it doesn't work

- **A board image is larger than expected.** It built at the default,
  `nros-relwithdebinfo`, which trades size for build speed. Use
  `nros-minsizerel` for a constrained board.
- **The profile you chose is not the one used.** Two platforms pin their own
  and say so: NuttX Rust images, and FreeRTOS images under QEMU, which miss
  zenoh-pico's handshake window at low optimization.
- **`error: profile '<name>' is not defined`.** A profile of your own needs a
  definition where cargo looks for one; see above.

## See also

- [Configure an image](configure-an-image.md#shrink-the-image-for-a-small-board):
  the size knobs beside the profile.
- [Build outputs](../../reference/build-outputs.md): where the `target/`
  directory of each profile lands, and the measured sizes per profile.
