# Phase 348 — Source-time provider discovery: buy the colcon convention, not colcon

**Status (2026-08-13). W1, W2 (rmw + boards), W3, W4 and W5 LANDED — the phase
is complete except the platform half of W2, which moves to
[phase-349](phase-349-rtos-integration-shells.md) W1 along with the naming
decision RFC-0072 settled.** Unblocked by
[phase-347](phase-347-rmw-as-a-declared-provider.md) W2 — descriptors exist, so
providers can describe themselves.

W1 shipped the announce mechanism, the scan, and `nros ws providers`. W2
migrated every provider that has a descriptor to agree with: **12 packages, 36
provisions** across rmw and boards. W3 added the index, the cmake seam that
reads it through the CLI, and the three-part cache invalidation. W4 derives
build ORDER from `<depend>`, adopted by all nine example workspaces.

Measured: **273 ms** to walk this repo (478 packages), **2 ms** to read the
index instead.

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
user's are. A **later root overlays an earlier one**, so a workspace package
shadows a nano-ros one (colcon's overlay-beats-underlay rule).

> Corrected in W5. This paragraph originally said "**first match wins**, so a
> workspace package shadows a nano-ros one", which is self-contradictory: the
> nano-ros tree is root 0, so first-match-wins means nano-ros always wins and a
> user's copy shadows nothing — the opposite of the stated workflow. The
> implemented rule is later-overlays-earlier. Note this is the reverse of
> `AMENT_PREFIX_PATH`, where the overlay is listed first; ours reads
> underlay → overlay so that `root[0]` names the same tree in every invocation.

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

## W2 — The migration — **rmw + boards LANDED; platform BLOCKED (see the correction below)**

The original table counted *directories*; most of those are support crates, not
providers. What is actually migrable is "has a descriptor", since the descriptor
is what a provision must agree with:

| axis | providers with a descriptor | migrated | provisions |
| --- | ---: | ---: | ---: |
| `packages/rmw` | 4 | **4** | 11 |
| `packages/boards` | 8 | **8** | 25 |
| `packages/platform` | 7 (in `config/`, see below) | 0 | — |

- [x] `packages/rmw/*` — all four backends.
- [x] `packages/boards/*` — all eight descriptor-carrying board packages.
- [x] `check-provider-announcements.py` (A1/A2) covers every family: a
      package.xml beside a descriptor announces provisions of that kind, and
      its names equal the descriptor's exactly, canonical first. Each case
      verified to FAIL under the matching perturbation.
- [ ] platform — **blocked on the descriptors being UNNAMED, not missing.**
      They live at `config/*/nros-platform.toml`, not `packages/platform/`.

Total: `nros ws providers` reports 36 provisions from 12 packages over 478
packages scanned.

**The gate is one script, not one per family.** This began as S5 inside
`check-rmw-descriptors.py`; when boards needed the identical rule it moved to
`check-provider-announcements.py` with a `FAMILIES` table rather than being
copied next to the board descriptors. Adding `platform` later is one row.
Copying a rule per family is the antipattern that turned #282 into #326.

### Platform is blocked — corrected 2026-08-11

> **This section originally claimed "there is no platform descriptor family;
> `nros-platform.toml` does not exist anywhere in the tree." That is wrong.**
> It exists — seven files, `config/<name>/nros-platform.toml` (RFC-0049). The
> search that produced the claim looked only under `packages/platform/`, by
> analogy with `packages/rmw/` and `packages/boards/`, and stopped there.
> Platform migration is still blocked, but for the two reasons below rather
> than for a missing family, and the difference matters: adding `names` to an
> existing descriptor is a much smaller job than designing one.

The family:

```
config/{bare-metal,freertos-lwip,generic,nuttx,posix,threadx,zephyr}/nros-platform.toml
```

**1. A platform descriptor declares no name.** Every rmw and board descriptor
carries an explicit `names = [...]`, and that list is exactly what
`check-provider-announcements.py` (A2) compares a provision against. A
platform's identity is instead its **directory name** — there is no `names` key
in any of the seven. So `<nano_ros_provides kind="platform" name="posix"/>`
would have nothing to agree with, and the rule that caught the `zenoh-pico`
mistake within an hour of being written would be vacuous for this family.

**2. Two vocabularies disagree, and neither is obviously authoritative.**

| board descriptor says `platform =` | config dir |
| --- | --- |
| `posix`, `bare-metal`, `nuttx`, `zephyr` | same name |
| `freertos` | `freertos-lwip` |
| `threadx-linux`, `threadx-riscv64` | `threadx` |
| `esp32` | **none** |
| — | `generic` (no board names it) |

`inherits` — the field that could bridge the two — is unset in all seven files.
So "which name does a platform provide?" has two candidate answers per platform,
and choosing between them is a design decision, not file-writing.

**What platform migration actually needs**, then: a `names` key in
`nros-platform.toml`, a decision on which vocabulary is canonical, and one row
in the gate's `FAMILIES` table. Not a new descriptor family.

**Both of those are now answered by
[RFC-0072](../design/0072-rtos-integration-nano-ros-is-a-guest.md)** (2026-08-12),
which was written to settle the `freertos` vs `freertos-lwip` question and
ended up settling this one too:

* the canonical vocabulary is the **board's** (`freertos`, not
  `freertos-lwip`), because the network stack is a fact the user declares
  rather than part of the platform's identity — `config/freertos-lwip/`
  becomes `config/freertos/` with `names = ["freertos", "freertos-lwip"]` so
  the old spelling still resolves;
* `esp32`'s missing platform file and `generic`'s dead `inherits` root are
  carried there as open questions rather than blocking here.

So the platform half of W2 is unblocked by [phase-349](phase-349-rtos-integration-shells.md)
W1, which adds the `names` key. It is deliberately NOT done in this phase: the
rename is only correct alongside RFC-0072's decision about what a platform is,
and doing it here would have meant renaming a directory on the strength of a
naming complaint.

Also worth recording, since both numbers appear above: the **8** board
descriptors are the authoritative set under `packages/boards/`, but the tree
holds **57** `nros-board.toml` files — the other 49 are the per-leaf
`.cargo/nros-board.toml` projections phase-341 W4 renders, which are outputs
rather than declarations. And only `zephyr` carries `[capabilities]`; the other
six are essentially `[build.zenoh]` blocks, so the family is thinner than
"seven descriptors" suggests.

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

## W3 — The index — **LANDED**

- [x] `nros sync` writes `<ws>/build/nros/providers.json` — the same build root
      as the SystemModels, gitignored, never committed. Written *before* the
      Rust-consumer block, which returns early for a C/C++-only workspace:
      placing it later would have skipped the index for exactly the workspaces
      with no cargo path to fall back on. A write failure warns rather than
      failing the sync, because the index is a cache.
- [x] CMake reads it **via the CLI**: `nano_ros_load_providers()` in
      `cmake/NanoRosProviders.cmake` runs `nros ws providers --lines` and gets
      TAB-separated rows. cmake never parses the index.
- [x] `--write-index` / `--index` / `--check-index`, and `--lines` for cmake.
- [x] Gated by `check-provider-index` (T1–T5), each case verified to fail under
      the matching perturbation.

Measured: **273 ms** for a fresh scan of this repo, **2 ms** to read the index —
136×, which is what makes the cache worth its correctness burden.

### Cache invalidation, which was indeed the fiddly part

Three mechanisms, because no one of them is sufficient:

1. **Watch the inputs.** The index records every package.xml read — providers
   *and* non-providers — and all of them go into `CMAKE_CONFIGURE_DEPENDS`
   (479 entries in this tree). A non-provider matters because *adding* a
   provision to one is precisely the edit that must re-configure.
2. **Rescan by default.** A watch list cannot contain a file that does not
   exist yet, so a newly-added provider is invisible to mechanism 1 — issue
   0196's exact shape. `nano_ros_load_providers()` therefore rescans every
   configure and refreshes the index as it goes; `REUSE_INDEX` opts into the
   2 ms read, so choosing staleness is deliberate rather than the default.
3. **Rescan and diff on demand.** `--check-index` compares a fresh scan to the
   index and exits non-zero naming each difference — new file, removed file, or
   a provision that appeared/vanished/moved.

**The roots are part of an index's identity.** An index built for a different
search path is *wrong*, not stale, so reading one is rejected with both root
lists rather than silently answering a question nobody asked. This is why
`nros sync` and `nros ws providers` resolve the search path through one shared
`provider_search_path()` — two spellings of "which roots" would make every
cached read fail.

### Not yet wired into a production configure

`nano_ros_load_providers()` has no caller in the build today; its first
consumer is W4, which needs exactly this to derive an order. It is exercised by
its gate rather than sitting untested. Calling it from the root `CMakeLists.txt`
now would add ~270 ms to every configure for a result nothing reads, so it
waits for the consumer.

## W4 — Ordering — **LANDED**

- [x] `nros ws order` derives a topological order from `package.xml`'s existing
      `<depend>` / `<exec_depend>` tags. No second dependency declaration was
      invented: entries already say
      `<exec_depend>talker_pkg</exec_depend>`.
- [x] `nano_ros_workspace(ORDER_FROM_DEPENDS)` reorders `SUBDIRS` through the
      CLI. Adopted by all nine `examples/workspaces/*`; all nine configure, and
      `c` builds clean.
- [x] Cycles and subdirs naming no package are hard errors, never a fallback to
      the authored order — silently building in the order someone typed is how
      a constraint stops being checked.
- [x] Gated by `check-workspace-order` (T1–T7), each case verified to fail under
      the matching perturbation.

*Acceptance, met:* `a_workspace_provider_is_ordered_before_its_consumer` plus
gate T2 — a `src/` holding a provider and its consumer orders the provider
first with nothing authored.

### The set stays authored; only the order is derived

That answers the wave's open question. A workspace's `SUBDIRS` list is filtered
by platform (`if(NANO_ROS_BOARD STREQUAL …) list(APPEND …)`), and which board is
active is a **selection** — no `<depend>` can express it. So discovery replaces
the half it can prove and leaves the half it cannot.

Proven load-bearing rather than assumed: with the C workspace's list reversed,
`ORDER_FROM_DEPENDS` configures clean while without it `nros codegen entry`
fails — exactly the constraint every workspace states as the comment "Node pkgs
BEFORE entries so the entry codegen sees their `nano_ros_node_register`
metadata".

### Ties break by the AUTHORED order, and this is what made adoption safe

The first implementation broke into ties by package NAME. It passed every
synthetic test and broke **four real workspaces** — `mixed`, `realtime-c`,
`realtime-cpp`, `realtime-cpp-subnode-portable`. Their entry packages declare no
`<exec_depend>` at all, so a name-sorted order was free to place `native_entry`
between `ctrl_pkg` and `telem_pkg` while violating nothing declared, and the
entry's codegen then ran before the node metadata it reads existed.

The premise "use package.xml's existing `<depend>`" is only as good as what the
packages actually declare, and half of them declare nothing.

So ties now prefer the caller's authored list, falling back to name. The sort
can only ever **move a package that a declared dependency requires moving** — it
fixes what is stated and preserves what is not. That turns this from a
replacement into a safety net, which is why it could be switched on for every
workspace at once instead of after a migration to declare all the missing deps.
Adding those `<exec_depend>` tags is still worth doing (the bringups'
`system.toml` names the packages, so the data exists) — it just is not a
prerequisite any more.

### Two bugs this wave found in W1/W3 code

* **`NANO_ROS_CODEGEN_TOOL` does not exist.** The tree's variable is the cache
  entry `_NANO_ROS_CODEGEN_TOOL`, resolved by `nros_bootstrap_codegen()`. W3's
  provider module used the un-prefixed name and its gate only passed because the
  harness set that name itself — a gate validating a fiction. Both modules now
  bootstrap the real one, and both gates pre-seed the cache variable real builds
  use.
* **A module dir captured as a normal variable is lost when the including frame
  pops** — the `_NROS_ENTRY_DIR` pattern (287-W6). Both modules now capture
  theirs `CACHE INTERNAL`.

## W5 — Shadowing — **LANDED**

- [x] `resolve_unique()` — a later root overlays an earlier one, and the losers
      are **kept and named** in `Resolution::shadowed` rather than dropped.
      `nros ws providers --resolve <kind>:<name>` prints the winner and every
      provider it shadows.
- [x] Ambiguity within one root is an error listing both candidates. Precedence
      *between* roots is defined; precedence *within* a root is not.
- [x] `candidates()` serves callers with their own discriminator — the W2
      finding that `threadx` is legitimately claimed by two board packages,
      separated by `target_contains = "riscv64"`. A flat "two packages, one
      name ⇒ error" rule would reject a shipping arrangement, so the unique
      resolver refuses while `candidates()` returns both.
- [x] An unknown name reports the names that *do* exist — a typo is the common
      case and the list is the fix.
- [x] Gated by `check-provider-index` T6–T8.

**Keeping the loser is ESP-IDF's lesson, not decoration.** It records both a
documented precedence order (`COMPONENT_SOURCE`) and the shadowed path
(`COMPONENT_OVERRIDEN_DIR`), noting that last-write-wins with no recorded
provenance would be unusable. "Why is my patched backend not being used" is
answerable only if the loser is still nameable.

**T6 initially passed for the wrong reason.** Both provider names appear in the
output whichever one wins, so `grep patched_backend` succeeded even with the
precedence deliberately INVERTED. Found by perturbation, not by review; the
check is now position-sensitive (winner line vs shadows line) and fails under
that inversion.

---

## Order

```
347 W2 ──► W1 ──► W2 ──► W3 ──► W4      (W1–W4 done)
                            └──► W5
```

W1–W3 turned out to be mechanical only in outline: W1 uncovered issue 0516, W2
found the board-name collision and the descriptor-only provider shape, and W3's
cache invalidation needed three mechanisms rather than a watch list, and W4's
premise held only where packages actually declare their deps. **W4 was the
phase's real
content** and should be scheduled as though it were the whole thing.

## Deliberately not here

* **Installed / binary providers.** Out of scope by the two-root rule above. If
  a vendor ever ships a prebuilt backend, that is a new decision with a new
  trade-off, not an extension of this phase.
* **Replacing colcon for user workspaces.** nano-ros does not build ROS 2
  packages; this is discovery of *nano-ros providers*, nothing more.
