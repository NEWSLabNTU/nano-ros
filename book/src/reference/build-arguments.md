# Build arguments

<!-- type: reference · level: 3 -->

The arguments `nros build` passes on to CMake and cargo, where each one can
be stated, the order they apply in, and what is refused. The CLI's own
flags are listed in the [CLI reference](cli.md).

## Where an argument can be stated

| Where | Spelling | Reaches |
| --- | --- | --- |
| The bringup's `system.toml` | `[image_defaults.build] cmake_args = ["-DFOO=ON", …]` | every image's CMake configure, and the source probe |
| The bringup's `system.toml` | `[image.<id>.build] cmake_args = ["-DFOO=ON", …]` | that image's CMake configure |
| The command line | `--cmake-args=<args>` | every CMake configure of this build |
| The command line | `--cargo-args=<args>` | every cargo command of a cargo image |
| The command line | `--profile=<name>` | the image's profile, for this build |

In `system.toml`, each list entry is one argument. There is no `cargo_args`
key: a feature a package needs goes in its own manifest.

The source probe is the CMake configure `nros build` runs once for the whole
workspace, before the build, to read each C and C++ package's metadata. It
reads only `[image_defaults.build] cmake_args`, so a path the probe needs,
such as a library's `-D<Lib>_DIR`, goes there. Changing that list runs the
probe again.

A CMake image or a west image ignores `--cargo-args=`, and a cargo image runs
no CMake configure, so it ignores `--cmake-args=`.

## How a value is read

| Form | Means |
| --- | --- |
| `--cmake-args="-DA=1 -DB='x y'"` | one value, split with plain POSIX quoting and no expansion: two arguments, `-DA=1` and `-DB=x y` |
| `--cmake-args=-DA=1 --cmake-args=-DB=2` | the flag repeats; the values add up in order |
| `--cmake-args=@args.txt` | one argument per line of `args.txt`, read from the current directory; blank lines and `#` comments are skipped, and the file cannot name another `@file` |
| `--cargo-args=<args>` | split, repeated and read from `@<file>` the same way |
| `--profile=dev`, `--profile=Debug` | what `[image.<id>] profile` takes: a cargo profile or a CMake build type |

The `=` is required on all three. The space-separated colcon form,
`--cmake-args -DA=1`, is refused, because it cannot tell a CMake argument
from the next `nros build` flag. The refusal shows the rewritten form:

```text
error: nros build takes `--cmake-args=<args>`, with `=`: the space-separated colcon form cannot tell a CMake argument from the next `nros build` flag.
  instead of: --cmake-args -DA=1
  write:      --cmake-args="-DA=1"
One value may hold many arguments, and the flag may be repeated.
```

## Order

The CMake arguments of one configure apply in this order, so a later one
wins:

1. `[image_defaults.build] cmake_args`
2. `[image.<id>.build] cmake_args`
3. each `--cmake-args=`, in the order given on the command line

## What is allowed

| Tool | Allowed arguments |
| --- | --- |
| CMake | `-D` (every spelling), `-U`, `-W*`, `--log-level`, `--log-context`, `--debug-find` |
| cargo | `--features` and `-F`, `--no-default-features`, `--all-features`, `-v`, `-q`, `--timings`, `--locked`, `--offline`, `--frozen` |

In a workspace with more than one package, name a feature with its package:
`--cargo-args=--features=<pkg>/<feature>`.

## What is refused, and where it goes instead

Each refusal names the argument and says where the setting belongs. From
`system.toml`, the same text starts with `[image.<id>.build] cmake_args:`.

| Argument | Why | Instead |
| --- | --- | --- |
| `-G`, `-T`, `-A`, `--toolchain` | the generator and the toolchain are the build's and the board's choice | change the image's `board` |
| `-C`, `-P` | they run a CMake script | — |
| `--preset`, `-S`, `-B` | the build lays out its own source and build trees | — |
| `-DCONFIG_*` | it sets a Zephyr Kconfig symbol around the Kconfig fragments and the knob ladder | the image's `conf = [..]` fragment |
| `-DCMAKE_BUILD_TYPE` | `nros build` passes it from the image's profile | `--profile=`, or `[image.<id>] profile` |
| `-DNROS_IMAGE`, `-DCMAKE_TOOLCHAIN_FILE`, and the other names `nros build` passes itself | the build sets them | the `system.toml` key the refusal names |
| a nano-ros knob, in any spelling (`-DNROS_FREERTOS_APP_STACK_KB`, its `CONFIG_NROS_*` or legacy name) | the image's knob table is its one committed place, and every build road reads it | `[image.<id>.knobs.<group>]`, named in the error |
| cargo `--release` | the profile is a build setting | `--profile=release`, or `[image.<id>] profile` |
| cargo `--target` | the target is the board's | change the image's `board` |
| a feature the build derives, such as `platform-posix` or `rmw-xrce` | the RMW, platform and capability features follow from `system.toml` | the image's `rmw`, `board` or capability in `system.toml` |

For example:

```text
Error: --cmake-args: `-DCONFIG_FOO=y` is refused: a `-DCONFIG_*` sets a Kconfig symbol directly on the west road, around the Kconfig fragments and the knob ladder; put Zephyr-native Kconfig in the image's `conf = [..]` fragment
```

A knob that means something on one platform only, such as
`[image.<id>.knobs.zenoh] socket_timeout_ms` (Zephyr), is refused when an
image on another platform states it, with the reason it does nothing there.
The image's `[image.<id>.knobs.<group>]` table, which
[Configure an image](../guide/build/configure-an-image.md) describes: it is
the one spelling every board reads.

## See also

- [Build profiles](../guide/build/build-profiles.md): what each profile
  means, and the default per board.
- [system.toml](system-toml.md): every key of the bringup's `system.toml`.
- [CLI reference](cli.md): `nros build` and its flags.
