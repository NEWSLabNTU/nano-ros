---
rfc: 0087
title: "Package identity and the provider format"
status: Draft
since: 2026-09
last-reviewed: 2026-09
implements-tracked-by: [phase-420]
supersedes: []
superseded-by: null
---

# RFC-0087 — Package identity and the provider format

## Summary

A nano-ros workspace is a colcon workspace: **a package is a directory containing
`package.xml`, and nothing else identifies one.** How to build it is declared by
`<build_type>`, and nano-ros-owned packages declare `nros_cmake` / `nros_cargo`
rather than borrowing ament's spelling. What a package *is* — an RMW, a board, a
platform, later a serializer — is announced by one export tag, because class
membership cannot be inferred from content. What a package *wants* is announced
by its mirror. Everything else in a provider descriptor is derived from
convention, and a "vendor package" is not a kind at all: it is an ordinary
package whose build fetches an external tree, exactly as `zenoh_cpp_vendor` is in
ROS 2.

The result is that the in-tree providers (`nros-rmw-zenoh`, the platform
packages, the boards) are found, selected and built through the same path a
user's own package takes. There is no builtin road.

## Motivation / problem

The machinery is mostly built already. `cargo_nano_ros::provider_scan` walks
`package.xml`, honours `COLCON_IGNORE` / `AMENT_IGNORE` / `NROS_IGNORE` /
`.nros-ignore`, extracts every `<depend>`, feeds `topological_order()`, and
`builder/discover.rs` unions that with Cargo members. Its own doc states the
principle this RFC is finishing: *"the nano-ros tree is simply the FIRST entry —
`packages/rmw/*` are not builtins reached by a different code path, they are
providers found the way a user's are."*

Five measured defects stand between that statement and reality.

**1. Seven spellings of `build_type`.** Counted across every `package.xml` in the
tree on 2026-09-04:

```
157  ament_cargo      5  ament_nros
125  ament_cmake      2  nros_entry
 75  cmake            1  nros_bringup
                      1  cargo
```

Three of those are already improvised nano-ros names, and two of them
(`nros_entry`, `nros_bringup`) encode a *role* rather than a build system.

**2. The colcon extension is keyed on a build type nothing declares.**
`colcon-cargo-ros2/setup.cfg` registers 15 build and 15 test tasks under
`ros.nros.<lang>.<platform>`, and `nros_augmentation/__init__.py` gates on
`desc.type.startswith("ros.nros.")`. colcon derives that type as `ros.` +
`build_type`. No `package.xml` in the tree declares `nros.<lang>.<platform>`, so
that path cannot fire.

**3. `ament_cargo` on an embedded package is a false claim.** It says a stock
`colcon build` can handle the package. For a Cortex-R5 firmware entry it cannot,
and the honest outcome is a refusal, not an attempt.

**4. Two readers have independently confused provision with consumption.**
`package_xml.rs` carries a test whose message is *"`<nano_ros rmw=…>` says what
this package CONSUMES; reading it as a [provision]…"*, and
`cmake/NanoRosPackageXml.cmake` records a reader bug that *"reported the file as
consuming `rmw=zenoh`. The file was correct; the reader…"*. Near-identical
spellings invited both.

**5. Descriptors write convention out longhand.** Of eleven fields in
`nros-rmw-zenoh/nros-rmw.toml`, six are derivable (`names` from the announcement;
`cargo_feature` = `rmw-<name>`; `cmake_value` = the name; `c_define_token` =
`UPPER(name)`; `cffi_feature` = `<cargo_feature>-cffi`; `crate` from the
package's `Cargo.toml`), one is non-derivable only by accident of history
(`cpp_define`, whose own descriptor comment says the spellings "are INCONSISTENT
across backends by history … and are preserved exactly"), and four are real
facts. Duplication of `names` is policed by a gate rather than removed, which is
the second-spelling shape CLAUDE.md warns about.

And one gap that is not a defect, only unbuilt: `default_search_path` returns
**exactly two roots, both inside the user's repo**, so a package tree living
anywhere else cannot be found.

## Design

### D1 — One recognition rule

A package is a directory containing `package.xml`. There is no second rule, and
in particular no rule keyed on `Cargo.toml` or `CMakeLists.txt`: a nano-ros node
or entry may be either cargo- or CMake-built, so the build file cannot identify
the package.

Cargo workspace members without a `package.xml` remain Cargo members and are not
packages. `builder/discover.rs` already needs both answers and neither is
authoritative for the other's question.

### D2 — `<build_type>` says how, and nano-ros-owned packages say so

Two new values, mirroring ament's pair:

| build_type | For |
| --- | --- |
| `nros_cargo` | nano-ros-owned package built through the cargo path |
| `nros_cmake` | nano-ros-owned package built through the CMake path |

**The class boundary is part of the decision:**

- Interface packages — `packages/interfaces/*` and a user's message packages —
  **keep `ament_cmake`**. They are genuinely ROS 2 packages; a ROS 2 node
  consumes their output, and claiming otherwise would be as wrong in the other
  direction.
- Entries, boards, RMW / platform / serdes providers, bringups — everything only
  nano-ros can build — take `nros_cargo` / `nros_cmake`.
- Standalone examples with no ROS identity keep plain `cmake` / `cargo`.

The consequence is the point: a stock `colcon build` refuses an `nros_*` package
with "unknown build type" instead of trying to install firmware into a prefix.
With `colcon-cargo-ros2` installed it builds normally, and defect 2 above closes
because the extension's entry points become `ros.nros_cmake` / `ros.nros_cargo`
— two keys instead of fifteen, and no new key per platform.

`ament_nros` folds into the pair. `nros_entry` and `nros_bringup` encode a role
that is already inferable (a bringup carries `system.toml`; an entry declares
one), so they fold in and declare nothing extra.

### D3 — Three export tags, one shape, one direction each

All live inside `<export>`, beside `<build_type>`.

```xml
<export>
  <build_type>nros_cargo</build_type>

  <!-- provision: "I am" -->
  <nano_ros_provides kind="rmw" name="zenoh"/>

  <!-- consumption, general form: "build me against" -->
  <nano_ros_uses kind="serdes" name="flatbuf"/>

  <!-- consumption, sugar for the common triple -->
  <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>
</export>
```

`<nano_ros_provides>` exists today (52 tags in 21 files: 33 board, 11 rmw, 8
platform). `<nano_ros …/>` exists today (91 files: `deploy` 90, `rmw` 51,
`board` 50). `<nano_ros_uses>` is new.

**The two directions are not unified, and the reason is `deploy`.** Provider
kinds are `rmw`, `board`, `platform` (and later `serdes`). `deploy` is *not* a
kind — it names a `[deploy.*]` block in `system.toml`, which
`NanoRosPackageXml.cmake` maps to the `NANO_ROS_PLATFORM` axis. So the
consumption tag is not provider selection under different attribute names; it
mixes provider selection with a system-model reference. Merging the tags would
flatten a distinction that is real in both cardinality (provision is 0..N —
zenoh declares three names; consumption is 0..1) and direction. Exactly one
package in the tree carries both tags (`nros-rmw-zenoh`), which is the proof they
must coexist rather than merge.

What *is* unified is their shape and their reader. `board="X"` is defined as
sugar for `<nano_ros_uses kind="board" name="X"/>`; `deploy=` stays an attribute
because it is not a kind. The payoff is that **a new provider family costs zero
new attributes**: selecting a serializer is `<nano_ros_uses kind="serdes"
name="flatbuf"/>`, with no change to either parser. Adding `serdes=` as an
attribute instead would mean teaching `NanoRosPackageXml.cmake` a fourth
attribute and `package_xml.rs` another special case.

One parser implements the rules for all three tags — inside `<export>`,
non-empty `kind`/`name`, comments stripped (issue 0516) — because both reader
bugs in the Motivation came from two readers implementing one rule separately.

### D4 — Descriptors carry only what cannot be derived

A provider announces its class and name in `package.xml`. The sibling descriptor
(`nros-rmw.toml`, `nros-board.toml`, `nros-platform.toml`, `nros-serdes.toml`) is
read only for the provider actually selected — one cheap parse per package, one
detailed parse per build — and holds only facts no convention can produce:
link libraries, `needs_cxx_linker`, capabilities, and each family's own axes.

Derived by convention, never authored in a new descriptor:

| Field | Derived from |
| --- | --- |
| the provider's names | the `<nano_ros_provides>` announcements, in order |
| cargo feature | `<kind>-<name>` |
| cmake value | the canonical name |
| C define token | `UPPER(name)` |
| cffi feature | `<cargo_feature>-cffi` |
| crate | the package's `Cargo.toml` |

**A stated derivable field must equal its derived value** — a ratchet, so the
existing rmw/board/platform descriptors are grandfathered where history forces a
spelling (`cpp_define`) and new drift is refused. Same shape as
`board-maintainer-baseline.json`.

Absent descriptor means "every default applies", which is the common case for a
small provider.

### D5 — Vendor packages are not a kind

A vendor package is an ordinary package whose build fetches and builds an
external source tree. Nothing marks it; ROS 2 marks nothing either. Putting it in
the source tree is the user's responsibility, which is colcon's contract.

```xml
<!-- spe_freertos_bsp_vendor/package.xml — the ASPIRATIONAL case: a clean
     upstream with no patch line, so a digest-pinned tarball is the right fetch.
     phase-418 W4 is where it would land. -->
<export><build_type>nros_cmake</build_type></export>
```

```cmake
FetchContent_Declare(spe_bsp
  URL      https://developer.download.nvidia.com/.../public_sources.tbz2
  URL_HASH SHA256=cd4fa3bd2bbd73af7bec6cc4e1e2ec179a8933a217c830d346a0a0c48ea90661)
```

**Amended 2026-09-05 from implementation (phase-420 W9): the fetch MAY be a
submodule, and for a patched upstream it MUST be.** This section read as though
`FetchContent` were the only spelling, and W9 measured what that costs.

`zenoh-pico` is a patch line — our fork, branch `nano-ros`, carrying commits we
authored. **A tarball has nowhere to put patches**, and a `PATCH_COMMAND` would
resurrect the `patches/` directory `.gitmodules` records as deliberately
deleted. The remaining spelling, `GIT_TAG <full sha>` at our own fork, satisfies
the invariant below and is *the identical pointer with `check-submodule-pins`
removed* — the gate that can go and ask the submodule, because a gitlink IS a
commit id. Converting would trade enforcement for uniformity.

So a fork is a vendor package whose fetch is a submodule, and that is the
finished shape rather than a way-station. `packages/rmw/zenoh/zpico-sys` is the
worked in-tree example: it owns the fetch (gitlink), the build
(`nros-zpico-build`) and the export (`links = "zpico"` → `DEP_ZPICO_*`), and W9
gave it the one thing it lacked — a `package.xml`, so a consumer can name it.

**The two pin gates partition the surface; they do not rank it.**
`check-submodule-pins` governs gitlinks, `check-vendor-fetch-pinned` governs
CMake fetch arguments, and a tree moving between mechanisms moves between gates.
Neither is the "real" one.

**A vendor package nothing depends on is a fixture, not a proof.** Do not add
one to demonstrate the shape; add one when a consumer needs the tree.

**Exports travel by each ecosystem's native channel, and nano-ros adds none:**

- CMake consumers see targets and cache variables, because the generated root
  `add_subdirectory`s the vendor package. No install prefix is involved.
- Cargo consumers use `links` + `cargo::metadata=` → `DEP_<LINKS>_<KEY>`. This
  tree already runs on that channel: `nros-node` declares `links = "nros_node"`,
  `nros-c`'s build script reads `DEP_NROS_NODE_RX_BUF_SIZE`, and codegen passes
  `DEP_NROS_MSGS_<PKG>_BOUNDS_JSON`.

That also removes a whole hazard class: a value that arrives through a dependency
edge cannot be poisoned by an ambient environment variable. The SPE BSP build in
`autoware-safety-island` failed exactly that way — an activated shell exported
`FREERTOS_DIR` pointing at a Cortex-M kernel and the vendor Makefile's `?=`
silently took it.

**Invariant:** a fetch without a digest is the same defect as an unpinned
submodule. Every `FetchContent_Declare` / `ExternalProject_Add` in a discovered
package carries `URL_HASH`, and any build script that downloads verifies one.
For a git fetch the digest is `GIT_TAG <commit>` — never a tag or a branch, both
of which are refs on a server we do not control, so upstream can change which
tree we build with no local diff (issue 1060). Keep the tag beside it, in a
comment or an index key, because it is what a human reads; and resolve the
commit with `git ls-remote <repo> refs/tags/<tag>`, taking the peeled `^{}` line
when there is one — an annotated tag's own sha is not a commit.

`check-vendor-fetch-pinned` scans the WHOLE tree rather than only the discovered
packages. Scoped strictly to the sentence above it would have reported OK on an
empty set while the tree's one fetch sat a directory up, outside every package —
`cmake/NanoRosCorrosion.cmake`'s Corrosion fallback, then at a movable
`GIT_TAG v0.6.1` (issue 1060). So it enforces inside packages and holds
out-of-package findings in a shrink-only baseline.

### D6 — The search path is an ordered list of roots

```toml
# nros.toml
[workspace]
package_paths = ["src", "~/nros-packages", "/opt/nros/packages"]
```

plus `NROS_PACKAGE_PATH`, colcon's `--base-paths` in source-time form. The
nano-ros tree stays first. **Shadowing is reported, never silent**: `nros ws
packages` prints each package's kind, the root it came from, and what it hid.

### D7 — Selection verbs

`nros build --packages-select <p>…` and `--packages-up-to <p>…`, with colcon's
semantics, over the topological order `provider_scan` already computes.

### D8 — What is deliberately not taken from colcon

- **Install prefixes and a sourced `setup.sh`.** `provider_scan`'s doc settles
  it: colcon's discovery artifact is the ament index, reached by sourcing
  `setup.sh`, which exists only after an install step — and nano-ros builds
  per-target static objects for RTOS targets with no install-and-source stage.
  Discovery stays source-time.
- **Per-package isolated builds for in-tree packages.** The merged cargo/cmake
  root is deliberate; a `--target-dir` serves exactly one workspace root (issue
  0616). Per-package build directories apply to vendor externals, where they are
  natural.
- **A Python plugin ABI.** Kinds are resolved in Rust; extensibility comes from
  descriptors and announcements, not entry points.
- **rosdep.** RFC-0062 already decided the dependency SSoT, and its 2026-08-29
  amendment deleted the rosdep fallback outright.

## Consequences

- The in-tree providers become indistinguishable from a user's, which is what
  makes the user-facing story testable: any gap in the provider path now breaks
  our own build.
- `nros build` remains offline. A vendor package needs network on its first
  configure, and that is once per HOST, not once per build directory — the
  shared cache landed with issue 1060 and lives at `$NROS_HOME/fetch` (default
  `~/.nros/fetch`, the sibling of the SDK store), overridable with
  `-DNROS_FETCH_CACHE=<dir>` or `NROS_FETCH_CACHE` in the environment and
  disabled with `OFF`; an unwritable location falls back to `<build>/_deps` with
  a message rather than failing. `cmake/NanoRosCorrosion.cmake` is the worked
  example.

  The mechanism is NOT a shared `FETCHCONTENT_BASE_DIR`, which is what this
  paragraph used to say. Measured: that variable moves all three of a
  dependency's directories, and the shared subbuild dir records the GENERATOR
  that populated it, so the second build tree on a different generator gets
  `CMake step for <dep> failed` — a hard error, not a slow path. What is shared
  is `SOURCE_DIR` + `SUBBUILD_DIR` (they must move together: ExternalProject's
  clone step keys on a stamp in the subbuild and `rm -rf`s the source before
  cloning, so a per-build subbuild destroys a shared source on every new build
  dir), while `BINARY_DIR` stays local — it is the `add_subdirectory` binary
  dir, and sharing it would be issue 0616 one layer over. Once the cache holds
  the pinned commit, `FETCHCONTENT_SOURCE_DIR_<uc>` — the per-dependency form of
  "do not download" — short-circuits population entirely: proven by configuring
  in a network namespace with no route, and again with `GIT_REPOSITORY` pointing
  at a path that does not exist. The project-wide
  `FETCHCONTENT_FULLY_DISCONNECTED` is deliberately not set; a provider has no
  business disconnecting its parent project's other dependencies.
- The serdes family (RFC-0088) costs one `FAMILIES` row and one descriptor
  schema, because this RFC did the general work.

## Gates

| Gate | Asserts |
| --- | --- |
| `check-build-type-spelling` | allowed values, and the D2 class boundary — a provider or entry may not declare `ament_*`; an interface package may not declare `nros_*` |
| `check-provider-announcements` | one row per family; announcements agree with descriptors where both exist |
| `check-derived-descriptor-fields` | a stated derivable field equals its derived value (ratchet) |
| `check-vendor-fetch-pinned` | every fetch in a discovered package carries a digest |

## Open questions

- Whether the bare `<nano_ros …/>` triple is eventually deprecated in favour of
  `<nano_ros_uses>` plus a `deploy=` attribute, or kept indefinitely as sugar.
  91 files argue for keeping it; symmetry argues the other way. Deferred until
  the general form has users.
- Whether `platform` descriptors move from `config/*/nros-platform.toml` beside
  their packages. They are the one family whose descriptor does not sit next to a
  `package.xml`, which D4's derivation assumes.

## Changelog

- 2026-09-04 — initial draft. Folds the packaging discussion: one recognition
  rule, `nros_cmake`/`nros_cargo`, three export tags with `<nano_ros_uses>` as
  the general consumption form, derived descriptor fields, vendor packages as
  ordinary packages with native export channels, search path, selection verbs.
