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

MITIGATED 2026-08-31 — the redirect stays, but it is now WITNESSED.

First the retirement question, since it is the better outcome and had not been
asked: is there a supported knob yet? No. Read against the pinned v0.6.1 AND
against upstream `master`: the same
`cmake_path(APPEND CMAKE_BINARY_DIR ${build_dir} cargo "<folder>_<hash5>")`,
still a plain local, still no cache variable, no `corrosion_import_crate()`
argument, no target property. So the symlink is not a workaround for a version
we are behind on — it is the only override point that exists, and "bump
Corrosion and use the knob" is not available.

What landed instead is `nros_assert_shared_cargo_dir_used()`
(cmake/NanoRosCorrosion.cmake) driving `scripts/check-shared-cargo-dir-used.sh`
as a build-time check on every Corrosion leaf that shares. It asserts the
RESULT, never the formula — a second copy of Corrosion's path rule would drift
from it silently, which is the defect rather than the fix:

  1. `${CMAKE_BINARY_DIR}/cargo` is still a symlink at the directory THIS
     configure chose;
  2. an artifact with the built target's name exists under that directory and
     its size matches the copy Corrosion produced.

Everything it consumes is documented or ours — `$<TARGET_FILE:...>` and
stat(2). It parses nothing.

Measured on `examples/native/c/talker` with sharing on: the healthy build prints
`shared-cargo-dir OK (nros_c-static)`, and all four dead-redirect states fail
with the artifact paths named. `ninja -t query` puts `libnros_c.a` above the
`||` line, so the witness re-runs on a real archive change and not on an
order-only edge (issue 0268's rule); a no-op rebuild does not re-run it.

WHAT IT STILL CANNOT CATCH, precisely: a long-lived build dir that shared
successfully BEFORE a Corrosion upgrade keeps a same-named artifact in place,
so with no code change the sizes can still match and this passes. It cannot
pass for long — any edit moves the size — and it cannot pass at all for a new
key, which is what a reconfigure after an upgrade produces. Byte-comparing
would close the hole and cost a full read of a multi-hundred-MB archive on
every leaf build; not worth it for a performance-regression detector.

Escape hatch: `NROS_ALLOW_UNSHARED_CARGO_DIR=1` downgrades the failure to a
warning, for someone mid-upgrade who wants the build to finish.

The witness's own negative control (five fixtures, including the state only the
symlink arm can catch) runs on the NORMAL path, not behind the flag — a witness
that had quietly stopped witnessing would be this very defect one level up.
`just check shared-cargo-dir-witness` (fast line) runs it standalone.

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

1. ~~**#1**, because it is the largest, the quietest, and it predates the
   campaign.~~ DONE 2026-08-31 — witnessed, not retired; Corrosion still
   offers no knob to retire it with. See item 1.
2. **#4**, the one still worth work. W5.c gave the Zephyr lane the OUT_DIR
   placer, so the W5 blocker is gone and the side channel has one fewer
   reader — but the WRITE is still there, because the other readers are not
   migrated: `NanoRosNodeRegister.cmake`, `NanoRosVerbs.cmake`, the px4
   integration module, and ~20 `just check` lanes that compile against
   `-Itarget/nros-c-generated`. Counted, not estimated: `git grep -n
   'nros-c-generated\|nros-cpp-generated'`. Deleting
   `write_header_to_target_dir` needs all of them moved first, which is a wave,
   not an afternoon.
3. **#2** only if the NuttX lane's nightly pin is ever revisited.
4. **#3** and **#5** are acceptable as-is: both fail loudly, neither gates CI.
