# Profiling Your Build

When a build feels slow, `nros-build-profile` tells you **where the time went** —
codegen vs compile vs link vs flash, which unit dominated, and whether a shared
crate was rebuilt redundantly.

It is a **passive, read-only** tool. Your build runs exactly as today on its native
toolchain (`west`, `cmake`, `idf.py`, `cargo`); the profiler only *reads* the timing
artifacts that build already produced. It never compiles or flashes anything, and it
adds no `nros` build/test command — `nros` stays setup + codegen.

## Installing it

`install.sh` puts it on your PATH beside `nros` (`~/.nros/bin/nros-build-profile`),
so there is nothing to install:

```bash
nros-build-profile --help
```

Release `0.6.0-nros4` and older ship it as source in the SDK root only;
install it from there once with cargo:

```bash
cargo install --locked --path "$(nros sdk-root)/packages/testing/nros-build-profile"
```

## How it works

Every backend already emits timing data:

| Backend | Artifact | Opt-in |
| --- | --- | --- |
| `west`, `cmake`, `idf.py` | `build*/.ninja_log` | none — ninja always writes it |
| `cargo` (native, esp32, cross) | `target*/cargo-timings/` | `cargo build --timings` |

The profiler discovers and parses these, then prints a stage table (always) plus an
optional per-unit drill-down and diagnostic hints.

## Flow 1 — Zephyr / cmake / esp32-idf (no opt-in)

```console
$ west build -b qemu_cortex_a53 my_app        # build/.ninja_log written
$ nros-build-profile my_app

Backend: ninja (west)        Total: 41.2s

Stage      Duration    %
codegen        1.1s   3%
compile       33.8s  82%
link           6.0s  15%

hints:
  - 1 unit = 61% of compile (libzenoh_pico.a, 20.6s)
```

## Flow 2 — cargo (one flag for per-crate detail)

The coarse table works with any build; the per-crate drill-down needs `--timings`:

```console
$ cargo build --timings --manifest-path my_talker/Cargo.toml \
      --config my_talker/build/linux/nros-cargo.toml   # build/linux/target/cargo-timings/ written
$ nros-build-profile my_talker/build/linux --deep

Backend: cargo               Total: 30.3s

Stage      Duration    %
codegen        1.2s   4%
compile       24.8s  82%
link           3.9s  13%

slowest units:
  zenoh-pico-sys     18.1s ########
  nros-node           2.4s #
  std_msgs            1.1s #
  <80 more>           3.2s

hints:
  - nros-c compiled 3× — examples use isolated target/; pool target_dir to reuse the build
```

If you build cargo without `--timings`, you still get the coarse table plus a note
telling you how to enable the drill-down — never an error.

## Flow 3 — machine-readable (CI regression diffing)

```console
$ nros-build-profile my_app --json
wrote my_app/nros-build-profile.json
```

A CI step can diff `nros-build-profile.json` across commits to catch build-time
regressions (a stage's seconds or a unit's share jumping).

## External (copy-out) projects

For any other project, run the installed bin the same way, on the
directory holding its `build*/` or `target*/`:

```console
$ nros-build-profile <project-dir> --deep
```

## Diagnostics

On top of the numbers, the profiler flags known slow patterns:

- a large native (C/C++/`-sys`) unit with no incremental → suggest a compiler cache;
- the same crate compiled more than once → suggest pooling `target_dir`;
- the configured job count vs available RAM (the fixture-build OOM budget).

Silence them with `--no-hints`.
