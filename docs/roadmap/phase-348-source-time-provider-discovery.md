# Phase 348 — Source-time provider discovery: buy the colcon convention, not colcon

**Status (2026-08-11). W1 + W2 (rmw, boards) LANDED. Platform migration blocked
on a missing descriptor family; W3–W5 open.** Unblocked by
[phase-347](phase-347-rmw-as-a-declared-provider.md) W2 (descriptors exist, so
providers can describe themselves). W2–W5 open.**

W1 shipped the announce mechanism, the scan, and `nros ws providers`. W2
migrated every provider that has a descriptor to agree with: **12 packages, 36
provisions** across rmw and boards.

Measured: 268 ms to walk this repo, 478 packages seen. That is per-invocation
cost with no caching — W3's index is where it stops being paid per configure.

**W1 turned up a defect older than this phase** ([issue
0516](../issues/0516-package-xml-regex-readers-blind-to-comments.md)): all
seven cmake readers of `package.xml` matched regexes against raw text, so a
COMMENTED-OUT element read as a declaration. The first provider `package.xml`
documents the provision-vs-consumption distinction in a comment, quoting the
other tag — and the reader then reported that file as consuming `rmw=zenoh`.
Fixed with one shared helper across all seven sites, gated by
`check-package-xml-comments`. Relevant to the rest of this phase: W2 writes
~40 more of these files, most of which will want exactly such a comment.

**Implements:** [RFC-0071](../design/0071-rmw-backend-descriptor.md) D5/D8, the
discovery half. phase-347 makes a provider *describable*; this phase makes it
*findable*.

**Why this is its own phase.** Its cost is a **migration**, not a mechanism, and
the hard part is not the scan — it is that discovery turns into scheduling.
Folding it into 347 would let a 3-line deletion and a topological build ordering
share an acceptance criterion.

---

## The convention, and the one place we diverge

Buy colcon's convention: the user drops packages carrying `package.xml` into
their workspace and the build finds them.

**Do not adopt colcon itself.** Its discovery artifact — the ament index reached
by sourcing `setup.sh` — exists only *after an install step*. nano-ros builds
per-target static objects for RTOS targets that generally have no dynamic
linking, so there is no install-and-source stage for an index to live in. That
is not an inconvenience to work around; it is why our discovery must be
**source-time**.

**One concept, no special cases:** an ordered list of workspace roots.

```
search path = [ <nano-ros root>, <user workspace> ]      # the default
```

The nano-ros tree is simply the **first entry** — `packages/rmw/*` are not
builtins reached by a different code path, they are providers found the way a
user's are. First match wins, so a workspace package shadows a nano-ros one
(colcon's overlay-beats-underlay rule).

Only these two roots are accepted, and both live in the user's repo. Rejected:
an installed index under `~/.nros`, and any env var such as `NROS_RMW_PATH` —
machine state makes a build irreproducible from the checkout and lets CI diverge
from a developer's box. Declining the installed case costs nothing today:
`packages/api/nros-cpp/CMakeLists.txt` has no `install()`, and the
`NanoRosCppTargets.cmake` its comments cite **is not in the tree**.

---

## W1 — Providers announce themselves — **LANDED**

- [x] `package.xml` gains a provision export, distinct in spelling from the
      existing consumption one: `<nano_ros_provides kind="rmw" name="zenoh"/>`,
      parsed by `PackageXml::parse` (`cargo-nano-ros/src/package_xml.rs`).
      Misplaced (outside `<export>`), empty, or unknown-attribute forms are
      hard errors — a provision that silently fails to register presents as
      "my backend isn't found", with nothing to grep for.
- [x] The scan (`cargo-nano-ros/src/provider_scan.rs`) reads only
      `package.xml`; `descriptor_path()` hands selection a path and never opens
      it. It prunes build/vendored trees, honours `COLCON_IGNORE` /
      `AMENT_IGNORE` / `NROS_IGNORE`, does not descend into a package (as
      colcon does not), and does not follow symlinks.
- [x] `nros ws providers [--workspace] [--nano-ros-root] [--kind] [--json]`.
- [x] `check-rmw-descriptors.py` S5: where a backend carries both, its
      provisions equal `[rmw].names` exactly, canonical first. It earned itself
      immediately — the first `package.xml` written claimed a `zenoh-pico`
      alias the descriptor does not have.

*Acceptance, met:* `scan_lists_providers_and_only_providers` covers both halves
(a package with the export is listed, one without is not) and the existing
build path is untouched — nothing reads the new tag but the new scan.

Two decisions worth carrying forward:

* **Root 0 falls back to the monorepo containing the `nros` binary.** Walking
  up from the workspace alone finds nothing for an out-of-tree consumer, which
  would drop every in-tree backend from the search path — useless for exactly
  the user this phase is for. The binary's location is reproducible from the
  checkout in a way an env var is not.
* **The scan reports facts, not policy.** Both sides of a shadowed name are
  returned with their `root_index`; deciding between them is W5. A scan that
  silently dropped the loser could not warn with both paths.

## W2 — The migration — **rmw + boards LANDED; platform BLOCKED**

The original table counted *directories*; most of those are support crates, not
providers. What is actually migrable is "has a descriptor", since the descriptor
is what a provision must agree with:

| axis | providers with a descriptor | migrated | provisions |
| --- | ---: | ---: | ---: |
| `packages/rmw` | 4 | **4** | 11 |
| `packages/boards` | 8 | **8** | 25 |
| `packages/platform` | 0 | — | — |

- [x] `packages/rmw/*` — all four backends.
- [x] `packages/boards/*` — all eight descriptor-carrying board packages.
- [x] `check-provider-announcements.py` (A1/A2) covers every family: a
      package.xml beside a descriptor announces provisions of that kind, and
      its names equal the descriptor's exactly, canonical first. Each case
      verified to FAIL under the matching perturbation.
- [ ] `packages/platform/*` — **blocked, see below.**

Total: `nros ws providers` reports 36 provisions from 12 packages over 478
packages scanned.

**The gate is one script, not one per family.** This began as S5 inside
`check-rmw-descriptors.py`; when boards needed the identical rule it moved to
`check-provider-announcements.py` with a `FAMILIES` table rather than being
copied next to the board descriptors. Adding `platform` later is one row.
Copying a rule per family is the antipattern that turned #282 into #326.

### Platform is blocked, and not on effort

**There is no platform descriptor family.** `nros-platform.toml` does not exist
anywhere in the tree. Platform *names* do exist — `posix`, `freertos`, `zephyr`,
`esp32`, `bare-metal`, `threadx-linux`, `threadx-riscv64` — but they live in the
`platform =` field of the *board* descriptors, which is a different package
declaring what a platform is called.

So announcing `<nano_ros_provides kind="platform" name="freertos"/>` on
`nros-platform-freertos` would be a hand-authored mapping with nothing to check
it against: exactly the second-source-of-one-fact that A2 exists to prevent, and
un-gateable by construction. Platform migration needs its descriptor family
first — the unfinished sibling of RFC-0042 — and that is a design decision, not
a wave of file-writing.

### Two findings that change later waves

**A board name may legitimately be claimed by two packages.** `threadx` is
declared by both `nros-board-threadx-linux` and `nros-board-threadx-qemu-riscv64`,
disambiguated by `target_contains = "riscv64"`. W5 below says "ambiguity within
one root is an error listing both" — that rule as written rejects a legitimate,
already-shipping arrangement. Amended: the *scan* reports both (it does), and
ambiguity is an error only when the candidates cannot be told apart by their
descriptors. This is the same facts-not-policy split W1 settled, arriving from
the other direction.

**A provider may have nothing to build.** `packages/boards/{linux,zephyr}/`
contain *only* `nros-board.toml`; the implementing crates are the siblings
`nros-board-linux` / `nros-board-zephyr`. Other boards (`nros-board-nuttx-qemu`)
keep the descriptor inside the crate. The package.xml has to sit beside the
descriptor, because that is where `descriptor_path()` looks — so discovery now
finds two shapes of provider, and **W4's topological build order must tolerate a
provider with no build step at all.**

## W3 — The index

- [ ] `nros sync` writes a provider index into `build/nros/`, beside the
      existing `nros-metadata.json` (`components`, `applications`).
- [ ] CMake reads it **via the CLI**, which is already the pattern —
      `NanoRosCodegenCore.cmake` shells out to `nros` 11 times. A second parser
      of the same file is the two-derivations defect this repo keeps paying for.
- [ ] Cache invalidation is the fiddly part: two source trees walked per
      configure, and the key is the `CONFIGURE_DEPENDS` class that has bitten
      before (issue 0196 — a probe that misses `generated/**`).

## W4 — Ordering, and this is the wave with teeth

A provider in `src/` may need **building before its consumer links it**, while
`nano_ros_workspace(SUBDIRS …)` takes an explicit list today. Discovery becomes
scheduling.

- [ ] Derive a topological order rather than an authored list.
- [ ] **Use `package.xml`'s existing `<depend>`** rather than inventing a second
      dependency declaration — the file is already being parsed, and a second
      source of the same fact is the defect W3 guards against, one level up.
- [ ] Decide whether the scan replaces `SUBDIRS` or supplements it. colcon has
      no equivalent of an explicit list, but the copy-out examples depend on
      being ordinary CMake projects, so an opt-out likely survives.

*Acceptance:* a workspace whose `src/` contains a provider AND a consumer of it
builds from a clean tree with no authored ordering.

## W5 — Shadowing

- [ ] A workspace provider overlaying a nano-ros one is a legitimate workflow
      (testing a patched backend). Allow it and **warn with both paths** —
      silently ignoring the user's copy is the worse failure.
- [ ] Ambiguity within one root is an error listing both — **but only when the
      candidates cannot be disambiguated by their descriptors.** W2 found
      `threadx` legitimately claimed by two board packages, separated by
      `target_contains = "riscv64"`. The flat "two packages, one name ⇒ error"
      rule would reject a shipping arrangement.

---

## Order

```
347 W2 ──► W1 ──► W2 ──► W3 ──► W4
                            └──► W5
```

W1–W3 are mechanical once the descriptors exist. **W4 is the phase's real
content** and should be scheduled as though it were the whole thing.

## Deliberately not here

* **Installed / binary providers.** Out of scope by the two-root rule above. If
  a vendor ever ships a prebuilt backend, that is a new decision with a new
  trade-off, not an extension of this phase.
* **Replacing colcon for user workspaces.** nano-ros does not build ROS 2
  packages; this is discovery of *nano-ros providers*, nothing more.
