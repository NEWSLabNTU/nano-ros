---
id: 1259
title: "`nros setup --tool zephyr-sdk-1-0-1` reports success and leaves an SDK
  with no toolchains -- the index pins the _minimal bundle and has no
  post-install step"
status: resolved
type: bug
area: tooling, zephyr
severity: medium
related: [issue-1254, issue-1258, phase-447]
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

## Resolution (2026-09-21)

Reproduced first, on `linux-x86_64`, against a scratch `NROS_STORE`:

```
$ nros setup --tool zephyr-sdk-1-0-1 --index nros-sdk-index.toml
nros setup --tool zephyr-sdk-1-0-1: prebuilt 1.0.1 (dist linux-x86_64) → <store>/sdk/zephyr-sdk-1-0-1/1.0.1
    … fetched zephyr-sdk-1.0.1_linux-x86_64_minimal.tar.xz: 70.7 MB in 19s (3.63 MB/s)
nros setup: 1 installed package(s) declare no `smoke` probe, so nothing measured whether they run: zephyr-sdk-1-0-1
EXIT=0
```

68 MB, `zephyr-sdk-1.0.1/{cmake,hosttools,sdk_gnu_toolchains,sdk_version,setup.sh}`,
no `gnu/`, exit 0 — exactly as filed. The one hint was the unprobed-package
line phase-447 C1 added, which says nobody looked rather than what is wrong.

### What changed

Two new `[tool.*]` keys, both facts about the ARTIFACT, stated in the row that
already names its URL and checksum (RFC-0014 §3a):

- **`post_install = { run, why }`** — the step that turns an UNPACKED prefix
  into a USABLE one. Run in the tool root by `sdk_store::run_post_install`,
  which `sdk_store::execute` calls — the same single pairing point
  `execute_and_probe` uses for the smoke probe, so all three install callers
  (`nros setup <board>`, `--tool`, the lazy `ensure_tools`) get it. The command
  is RECORDED in `.nros-provenance`, and only on success: `plan_install`
  compares the recorded command against what the index states today, so a
  repeat run is `Present`, a prefix whose completion failed is
  `InstallAction::Complete` (resumed at that step, nothing re-downloaded), and
  a prefix unpacked before this key existed is completed rather than left
  broken.
- **`subdir`** — the tarball's own top-level directory, with `{version}`
  substituted from the pin. One spelling, `sdk_index::tool_root`, reached by
  `ToolPackage::root_of`; it is the base for a `smoke` argv, for
  `post_install`'s working directory, for a `front` entry, and for
  `nros sdk-path`.

`nros sdk-path <tool>` therefore answers the SDK **root**, and
`scripts/lib/zephyr-sdk.sh` stops appending `zephyr-sdk-<version>` by hand —
the second copy of the pin this issue's third bullet was about.
`sdk_store::tool_install_prefix` is the separate question "what did setup write
and what does `store gc` reclaim", so the gc entry stays the whole versioned
directory.

Both SDK entries now declare `post_install` (`./setup.sh -t arm-zephyr-eabi -t
x86_64-zephyr-elf -h`) and real `smoke` probes, so the smoke-or-reason ratchet
SHRANK by two. The two rows are NOT symmetric, and the index says which half
does the work in each: on `zephyr-sdk-1-0-1` the `-t` arguments are the fix
(they fetch the toolchains the `_minimal` bundle omits), while on `zephyr-sdk`
every toolchain is already in the 1.3 GiB bundle and upstream's loop skips a
directory that exists, so there the live half is `-h` (the bundled host-tools
installer, which nothing on this road ran before) and the `smoke` probes are
what the row gains most from. `-c` is deliberately NOT in either: it writes
`~/.cmake/packages/Zephyr-sdk/<hash>`, which is per-USER state outside the
prefix — the accumulation this issue's first bullet measured.
`scripts/zephyr/ensure-sdk-registered.sh` still owns registration (issue 1279)
for the callers that want it.

### The class, and the gate

The reported site was one `[tool.*]`; the class is "an install path that
reports success without measuring anything". The sweep over the sibling arms is
`.config/smoke-or-reason-baseline.txt`, which already enumerates every
`[tool.*]` with no probe and its reason. The zephyr-sdk pair's reason was filed
under "structural", and it was wrong twice: its first half
("a prefix-relative argv cannot reach `<prefix>/zephyr-sdk-<version>/` without
restating the pin") was an argument about the SCHEMA, which `subdir` changes;
its second half ("the toolchains are not even present until `setup.sh -t`
fetches them") was a description of THIS BUG. The other nine reasons — one
structural (`corrosion`, which installs CMake package files and no executable)
and eight debt — are unaffected.

The second sweep is over the rows this could have bitten: `[tool.*]` entries
pinning an UPSTREAM artifact rather than one we repack, i.e. the ones whose
layout and completeness we do not control.

```
$ python3 - <<'EOF'   # (tomllib over nros-sdk-index.toml)
for n, t in sorted(index["tool"].items()):
    if any("NEWSLabNTU/nano-ros-sdk" not in d["url"] for d in t.get("dist", {}).values()):
        print(n, t.get("subdir"), bool(t.get("post_install")), len(t.get("smoke", [])))
EOF
arm-fvp  None False 0        ninja             None                    False 0
cargo-llvm-cov  None False 1  sccache          None                    False 1
cargo-nextest   None False 1  verus            None                    False 1
clang-format    None False 1  zephyr-sdk       zephyr-sdk-{version}    True  2
espflash        None False 1  zephyr-sdk-1-0-1 zephyr-sdk-{version}    True  2
mdbook          None False 1
```

Eleven rows. Nine of them are single-binary archives normalised into the mirror
shape by a dist `install` step (`tar -xOf {archive} … > {prefix}/bin/<x>`), so
the prefix IS the root and there is no installer to run; seven already carry a
probe and the two that do not (`arm-fvp`, `ninja`) carry baseline reasons that
this change does not touch. Only the two SDK rows are multi-component bundles
with an installer of their own — which is why `subdir` and `post_install` land
on exactly those two and nothing else moves.

`check-smoke-or-reason` gains the rule that makes this stick: **a tool
declaring `post_install` may not be argued away, only probed.** The completion
step's only other witness is its exit status, and upstream's installer exits 0
having skipped a toolchain whose directory already exists — correct, and
indistinguishable from a skip that fetched nothing. Both directions are in the
gate's self-test.

### Measured after

Resuming the very prefix the reproduction left behind (so the `Complete` arm is
what ran, and nothing was re-downloaded):

```
nros setup --tool zephyr-sdk-1-0-1: completing 1.0.1 (unpacked; running the SDK's own
  setup.sh: fetch the arm-zephyr-eabi + x86_64-zephyr-elf toolchains the _minimal bundle
  omits, and install host tools (~1 GiB, several minutes)) → <store>/sdk/zephyr-sdk-1-0-1/1.0.1
    … zephyr-sdk-1-0-1: running the SDK's own setup.sh: …
Zephyr SDK 1.0.1 Setup
Installing 'arm-zephyr-eabi' GNU toolchain ...
Installing 'x86_64-zephyr-elf' GNU toolchain ...
Installing host tools ...
All done.
EXIT=0
```

- **2.2 GB**, with `gnu/{arm-zephyr-eabi,x86_64-zephyr-elf}` — the size the
  issue measured by running the installer by hand.
- `.nros-provenance` carries
  `post_install = "./setup.sh -t arm-zephyr-eabi -t x86_64-zephyr-elf -h"`.
- A second `nros setup --tool zephyr-sdk-1-0-1` prints `present 1.0.1 (skip)`.
- `nros setup --tool zephyr-sdk-1-0-1 --check` prints `[OK]`.
- `nros sdk-path zephyr-sdk-1-0-1 --require` prints
  `<store>/sdk/zephyr-sdk-1-0-1/1.0.1/zephyr-sdk-1.0.1`, and
  `scripts/lib/zephyr-sdk.sh resolve` prints the same path it did before.
- Negative control — with `gnu/arm-zephyr-eabi` moved aside, `--check` reports
  `[BROKEN] … gnu/arm-zephyr-eabi/bin/arm-zephyr-eabi-gcc --version does not
  work` and exits 1. The probe measures something.

`[tool.zephyr-sdk]` (0.16.8, the `[board.zephyr]` entry and the 3.7 line) was
measured on the same host, from nothing: 1,445 MB fetched in 11m01s, host tools
installed, **7.8 GB** after, `--check` `[OK]`, a second run `present 0.16.8
(skip)`. That run is also what corrected this entry's comment — `setup.sh -t` in
0.16.x is a `[ -d ] && continue` over directories the unpack already made, so
the `-t` arguments there are a no-op and `-h` is the live half. An earlier draft
of that comment claimed `-t` rewrote sysroot paths; it does not, and the
download is what said so.

### Not verified here

`nros setup zephyr` was exercised as far as `--dry-run` and as the `[tool.*]`
install it resolves; the acceptance line "produces an SDK a 4.4 `west build`
accepts" was NOT run, because no 4.4 Zephyr workspace is provisioned in this
worktree. What is measured is one step short of it: both SDKs install, both
smoke-probe green, and the probes are `arm-zephyr-eabi-gcc` / `x86_64-zephyr-elf-gcc`
— the two toolchains `FindZephyr-sdk.cmake` was failing to find.

### Not closed here

The issue's remaining "also found" bullets are separate work and stay open as
such: there is still **no local source for a dist** (a `file://` override
verified against the same sha256 would make a host-to-store move a copy rather
than a re-download), and the two SDK entries are **still not named by one
rule** (`zephyr-sdk` is 0.16.8, `zephyr-sdk-1-0-1` is 1.0.1), so a consumer
starting from `zephyr/SDK_VERSION` searches the index by version instead of
composing a name. `scripts/lib/zephyr-sdk.sh` is the one resolver for that
mapping today.
