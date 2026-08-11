# Phase 348 — Source-time provider discovery: buy the colcon convention, not colcon

**Status (2026-08-11). W1 LANDED — unblocked by
[phase-347](phase-347-rmw-as-a-declared-provider.md) W2 (descriptors exist, so
providers can describe themselves). W2–W5 open.**

W1 shipped the announce mechanism, the scan, and `nros ws providers`. One
provider (`nros_rmw_zenoh`) carries the export as the acceptance proof; the
rest of the migration is W2 and can proceed one package at a time, because a
provider without a `package.xml` is simply not discoverable and its existing
build path is untouched.

Measured: 268 ms to walk this repo, 472 packages seen, 1 provider. That is
per-invocation cost with no caching — W3's index is where it stops being paid
per configure.

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

## W2 — The migration, which is the bulk of the phase

No nano-ros provider carries a `package.xml` today. The 99 in the tree are
interface packages and test fixtures.

| axis | dirs needing `package.xml` | carry a descriptor today |
| --- | ---: | ---: |
| `packages/rmw` | 8 families | 0 (phase-347 W2 adds them) |
| `packages/boards` | 17 | **8** |
| `packages/platform` | 14 | — |

- [ ] Add the export per provider, incrementally.
- [ ] **Nothing is deleted before its replacement covers the same set.** A
      provider without a `package.xml` is simply not discoverable by the scan,
      and its existing path keeps working — so the migration can stop half-done
      without breaking anything.

Note the descriptor family is itself roughly half-populated (8 of 17 boards), so
this wave also finishes what RFC-0042 started.

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
- [ ] Ambiguity within one root (two packages claiming one name) is an error
      listing both.

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
