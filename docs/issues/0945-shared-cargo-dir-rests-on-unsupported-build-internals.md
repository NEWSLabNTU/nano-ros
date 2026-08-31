---
id: 945
title: "The shared-cargo-dir campaign rests on five unsupported build-system
  internals — a Corrosion path formula, an unstable cargo flag, cargo's private
  `.fingerprint` format, a side channel inside cargo's target dir, and an
  undocumented depfile location"
status: open
type: tech-debt
area: build
related: [issue-0805, issue-0616, issue-0499, issue-0834, issue-0112]
---

## Symptom

Nothing is failing. This is a register of what the build-performance campaign
(phase-400, and issue 0805 before it) DEPENDS ON that no one has promised to keep
working. Each item breaks on somebody else's release, not on a change of ours,
and most break QUIETLY — the build keeps going and stops sharing, or keeps
sharing and reads the wrong file.

Filed because the exposure was reviewed once, deliberately, and that review
should not have to be reconstructed from commit messages.

## The five

### 1. The Corrosion symlink redirects a path Corrosion computes privately

`nros_share_corrosion_cargo_dir()` (cmake/NanoRosSharedCargoDir.cmake) works by
symlinking over `${CMAKE_BINARY_DIR}/cargo`, because Corrosion derives its
`--target-dir` as `${CMAKE_BINARY_DIR}/cargo/<workspace-folder>_<hash-of-manifest-path>`
and, in its own words there, *"Corrosion 0.6.1 exposes no knob for the directory
(it is a plain local)"*.

RISK: a Corrosion upgrade that moves or renames that path leaves the symlink
pointing at nothing, or at a directory Corrosion no longer uses. Sharing then
stops silently — the build still succeeds, just slower, and the only signal is a
number nobody is watching.

BLAST RADIUS: six platforms (freertos, native, nuttx, qemu-baremetal,
threadx-linux, threadx-riscv64). This is the campaign's single largest
dependency on someone else's internals, and it PREDATES phase-400.

MITIGATION IF IT MATTERS: assert at configure time that the symlink target is
the directory cargo actually writes to — one `find` for a known artifact after
the first build, compared against the link — so a Corrosion move fails loudly
instead of degrading.

### 2. `--artifact-dir` is an explicitly unstable cargo flag

The NuttX FFI driver (packages/api/nros-c/cmake/nros-nuttx.cmake) passes
`-Z unstable-options --artifact-dir` to evict per-leaf artifacts from a shared
target dir. It survives only because that crate is pinned to a nightly
toolchain, and the flag has already been renamed once (`--out-dir`).

RISK: a rename or a semantics change breaks the NuttX lane at the point where it
copies its kernel ELF. Loud, at least.

NOTE: this is also why the Zephyr C/C++ lane cannot use the same eviction —
native_sim builds on STABLE, so the flag is unavailable there. That constraint is
what forces features into the shared-dir key and caps the Zephyr collapse at
70 -> 28 build dirs instead of 70 -> 14 (phase-400 W5).

### 3. `just leaf-graph` and `just shared-dir-churn` parse cargo's private
`.fingerprint` format

Both tools read `<target-dir>/**/.fingerprint/<unit>/*.json` — an on-disk format
with no stability guarantee and no documentation. `deps`, `features`,
`compile_kind` and the `local` array of `RerunIfEnvChanged` / `RerunIfChanged`
entries are all internal.

RISK: a cargo release changes the schema and the tools return wrong answers
rather than errors — the shape that already cost this phase real time when
`leaf-graph` reported a dependency that had been removed.

WHY IT WAS STILL WORTH BUILDING: the question they answer ("what did THIS build
compile, and who required it?") has no supported interface. `cargo tree`
re-resolves the workspace and answers a different question, which is exactly the
mistake these tools exist to prevent. The honest position is that they are
DIAGNOSTIC tools, not gates: nothing in `check-fast` depends on them, and nothing
should.

PARTIAL MITIGATION ALREADY IN PLACE: both have `--self-test`, so a schema change
that breaks parsing surfaces as a failing self-test rather than a plausible wrong
number.

### 4. The generated headers are a side channel inside cargo's target dir

Build scripts write `$CARGO_TARGET_DIR/nros-{c,cpp}-generated/nros/*.h` — a path
INSIDE cargo's tree that cargo does not manage. It works because nothing cleans
it, not because it is supported.

RISK: any future cargo target-dir GC treats those files as unowned. And it is
already the direct cause of the W5 blocker: share the target dir and the second
image takes a cache hit, the build script never re-runs, and the directory the
consumers were pointed at is never written.

BEING ADDRESSED: phase-400 W5.c emits the same headers to `$OUT_DIR` — cargo's
sanctioned location, per-unit and hashed BY CARGO, so two feature sets cannot
collide without us keying anything. Measured: default features and
`rmw-cffi,platform-posix,std,ros-humble` land in different `OUT_DIR`s. The path
is discoverable on the stable JSON stream (`{"reason":"build-script-executed",
… "out_dir": …}`), which cargo emits even on a FULLY CACHED run (measured: 13
events with nothing to rebuild) — the property the side channel lacks.

The side-channel write stays for now; consumers migrate to the OUT_DIR copy, and
the side channel can be deleted once none remain.

### 5. The probe depfile's location is an undocumented layout convention

`nros-sizes-build::probe_depfile` knows that cargo writes a rustc depfile beside
the UPLIFTED artifact and never beside the hashed `deps/` copy. Measured in this
repo's probe store: 182 uplifted rlibs with 182 depfiles, 269 `deps/` rlibs with
none.

RISK: a layout change silently removes the watch list, which is issue 0563's
defect (a probe that measures a crate and then does not watch it).

MITIGATION ALREADY IN PLACE: the lookup tries both spellings and PANICS if
neither exists, so the failure is loud and names the file. Pinned by a unit test.

## Not on this list, deliberately

`nros_shared_cargo_dir()` itself (a directory plus a SHA1), the fixture stamp and
its `.started` sidecar (our file, our format), the `nros-sizes-build` nested cargo
using `--message-format=json` (stable, documented), and `rerun-if-env-changed` /
`rerun-if-changed` (documented). These are ours or supported, and carry no
version exposure.

## Suggested order if this is picked up

1. **#1**, because it is the largest, the quietest, and it predates the campaign.
2. **#4**, already in progress — finishing it deletes a whole class.
3. **#2** only if the NuttX lane's nightly pin is ever revisited.
4. **#3** and **#5** are acceptable as-is: both fail loudly, neither gates CI.
