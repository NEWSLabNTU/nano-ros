---
id: 1259
title: "`nros setup --tool zephyr-sdk-1-0-1` reports success and leaves an SDK
  with no toolchains -- the index pins the _minimal bundle and has no
  post-install step"
status: open
type: bug
area: tooling, zephyr
severity: medium
related: [issue-1254, issue-1258]
---

## Symptom

```
$ nros setup --tool zephyr-sdk-1-0-1
nros setup --tool zephyr-sdk-1-0-1: prebuilt 1.0.1 (dist linux-x86_64) -> ~/.nros/sdk/zephyr-sdk-1-0-1/1.0.1
```

exits 0 and leaves 68 MB:

```
zephyr-sdk-1.0.1/{cmake,hosttools,sdk_gnu_toolchains,sdk_version,setup.sh}
```

No `gnu/arm-zephyr-eabi`, no `gnu/x86_64-zephyr-elf`. The first `west build`
for any Cortex-M board fails inside `FindZephyr-sdk.cmake`, naming neither the
missing toolchain nor the step that should have fetched it.

## Cause

`[tool.zephyr-sdk-1-0-1]` pins sdk-ng's `_minimal` tarball. A minimal bundle
is DESIGNED to be completed by its own installer, `./setup.sh -t <target> -h
-c`, which downloads the toolchains, installs the host tools and registers the
CMake package. The index has no post-install concept (no such key in
`nros-sdk-index.toml`, nothing in `cmd/setup.rs`), so `nros setup` stops at
unpack.

nano-ros's own `scripts/zephyr/setup.sh` hides this by running the installer
itself (`install_sdk`) -- but only on the path that installs the SDK with
`--prefix` INSIDE the checkout (issue 1254). The store path, which RFC-0095
makes the default, is the one that never completes.

`[board.zephyr]` lists `packages = ["zephyr-sdk"]`, the 0.16.8 entry, which
pins the FULL bundle, so `nros setup zephyr` on the 3.7 line ships toolchains;
the installer's host-tools and registration steps are still skipped there.

Measured on this host: after running the installer by hand in the store entry,
2.2 GB with `gnu/{arm-zephyr-eabi,x86_64-zephyr-elf}`, and the SDK accepted by
a Zephyr 4.4 board build.

## Also found on the same path

- **The per-user CMake package registry accumulates SDKs from every clone.**
  The installer's `-c` writes `~/.cmake/packages/Zephyr-sdk/<hash>`. On the
  host this was measured on it lists five SDKs from four trees: three
  separate nano-ros clones, each with its own `scripts/zephyr/sdk/`, and the
  store. A configure without `ZEPHYR_SDK_INSTALL_DIR` gets whichever
  `find_package(Zephyr-sdk)` picks, which is a silent version substitution of
  the kind `nros sdk-path` exists to prevent (issue 0625).
- **`nros sdk-path` answers the store PREFIX, not the SDK root.** The tarball
  has a top-level `zephyr-sdk-<ver>/` and is unpacked without
  `--strip-components`, so `ZEPHYR_SDK_INSTALL_DIR` must be
  `<prefix>/zephyr-sdk-<ver>`. Every consumer has to know the tarball layout.
- **There is no local source for a dist.** `nros setup --tool` fetches the
  index URL or nothing (`NROS_OFFLINE` only drops the system provider). Moving
  a host to the store therefore re-downloads an SDK it already holds: the full
  0.16.8 tarball arrived at ~400 KB/s (121 MB in the first ~5 minutes) with an
  installed 0.16.8 one directory away. A `file://` dist override, verified
  against the same sha256, would make that a copy.
- **The SDK entries are not named by one rule** (`zephyr-sdk` is 0.16.8,
  `zephyr-sdk-1-0-1` is 1.0.1). A consumer that starts from the version a
  Zephyr tree states in `zephyr/SDK_VERSION` has to search the index by
  version rather than compose a name.

## Fix shape

- The index can state a post-install command for a tool (the installer and its
  `-t` targets), and `nros setup` runs it, recorded in `.nros-provenance`, so a
  re-run is a no-op.
- Registration (`-c`) is dropped in favour of consumers exporting
  `ZEPHYR_SDK_INSTALL_DIR` from `nros sdk-path`.
- `sdk-path` returns the SDK ROOT (the index records the subdirectory), and
  the Zephyr line -> SDK entry mapping has one resolver.

## Acceptance

- `nros setup zephyr` alone produces an SDK a 4.4 `west build` accepts.
- A second `nros setup` is a no-op, and `nros store gc` reclaims the whole
  entry.
