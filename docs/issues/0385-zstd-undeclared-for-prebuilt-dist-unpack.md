---
id: 385
title: "`nros setup` cannot unpack a zstd prebuilt dist on a host without `zstd`, and reports only `unpack prebuilt archive`"
status: resolved
type: bug
area: build
related: [issue-0368, rfc-0014, rfc-0062, phase-327]
---

## Resolution (2026-08-02)

Both defects fixed:

- **D1 (undeclared):** added `[system.zstd]` to `nros-sdk-index.toml`
  (apt/dnf/pacman/brew `zstd` + `check = { cmd = "zstd" }`). `nros setup
  --system` / doctor now list it (verified: with zstd masked, `--system
  --check` shows `[MISSING] zstd` and composes it into the install command).
- **D2 (opaque error, probe-late):** `sdk_store::execute`'s prebuilt arm probes
  `zstd` on PATH BEFORE downloading when the dist URL is `.zst`/`.tzst`, and
  bails with the package name for the detected manager (reusing
  `detect_package_manager` + `native_install_command`, now `pub(crate)`):
  *"this prebuilt dist is zstd-compressed, but the `zstd` binary is not on PATH
  … Install it: sudo apt-get install -y zstd (or run `nros setup --system`)"*.
  Verified: with zstd masked, `nros setup --tool cyclonedds` fails with that
  message before any download.
---

# zstd is an undeclared prerequisite of every prebuilt dist

## Symptom

On a stock Ubuntu 22.04 (no `zstd` package — it is not in the base image),
provisioning the Cyclone backend fails:

```
$ nros setup native --rmw cyclonedds
nros setup: native (rmw cyclonedds) needs 2 package(s):
  cyclonedds             prebuilt 0.10.5-nros1 (dist linux-x86_64)
tar (child): zstd: Cannot exec: No such file or directory
tar (child): Error is not recoverable: exiting now
Error: install cyclonedds 0.10.5-nros1

Caused by:
   0: unpack prebuilt archive
   1: `tar -xf …/0.10.download -C …/0.10.5-nros1` failed (exit status: 2)
```

`sudo apt-get install zstd` fixes it completely.

## Two defects

**D1 — the dependency is undeclared.** The dists are `.tar.zst`, so `tar` needs
the external `zstd` binary. Nothing declares it: not the `[system.*]` block in
`nros-sdk-index.toml` (which exists precisely for this class since phase-327 /
issue 0368 F3), not `just doctor`, not the book's host-prerequisites section.
Arch and Fedora ship `zstd` in their base systems, and the CI images have it, so
it is invisible from every machine the project routinely uses — a stock Ubuntu
22.04 container is where it surfaces. Note this affects the FIRST prebuilt any
user installs, not an exotic board.

**D2 — the error names no remedy.** The `tar (child): zstd: Cannot exec` line is
tar's, printed to stderr and easy to miss; what `nros setup` itself reports is
`unpack prebuilt archive` plus a `tar -xf` command line. A user has to know that
`.download` is zstd-compressed to connect the two. Issue 0368 F3 made the same
point about the qemu dist's `libslirp` runtime dep: the failure surface should be
"install this package", not a bare loader/tar error.

## Direction

1. Add a `[system.zstd]` entry (apt `zstd`, dnf `zstd`, pacman `zstd`, brew
   `zstd`, `check = { cmd = "zstd" }`) and attach it to every tool whose dist is
   `.tar.zst` — or, better, treat it as a prerequisite of the store's unpack
   step itself, since it applies to all of them rather than to one package.
2. Probe before downloading. `install_single_tool` already probes `tool.system`
   deps BEFORE doing any work, for exactly this reason ("rather than a dead
   configure 40 minutes into a source build"); the unpack path should get the
   same treatment, so the failure comes before a 1 MB download rather than after.
3. Map the decompressor from the archive's suffix and, when it is missing, fail
   with the package name for the detected package manager — `nros setup --system`
   already knows how to print that command.

## Evidence

Ubuntu 22.04.5 distrobox on an Arch host, checkout `c0aad42d8`, box store at
`$NROS_HOME=~/.nros-ubuntu`. The zenoh RMW path provisioned cleanly beforehand
because zenohd is source-built on this host (issue 0374) and never touches the
unpack path — so the gap only appears on the first PREBUILT dist a host pulls.
