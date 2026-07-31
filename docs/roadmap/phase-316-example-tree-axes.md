# Phase 316 — every example path level names a real axis

**Status (2026-07-31): W1–W3 landed and pushed. W4.1 and W4.3 DECIDED (two demos: direct + build-time-RMW bridge); W4.2/W4.3
recommended for a SUCCESSOR PHASE — scoping found no nano-ros node has ever been
built on the uORB backend, and there is no px4 platform cmake module, so W4 is a
phase rather than a work item.**
**Implements:** RFC-0026 (example directory layout).
**Closes:** issue 0315. **Informed by:** issues 0314, 0319, archived 0295.
**Filed on the way:** issue 0356 (`px4_e2e.rs` targets a tree retired by
phase-277 W7).

`check-example-matrix.sh`'s `allowed_roots` is **empty** and its `is_allowed()`
px4 branch is gone — the acceptance condition for W1+W2 together.

## Goal

Make RFC-0026's rule true as written — *"there are no `<rmw>/` siblings"* —
by removing the last three `<rmw>/` levels, and end
`scripts/check-example-matrix.sh`'s carve-out list.

Two of the three are not RMW levels at all. px4's `uorb/` and `xrce/` encode
**where the code runs** (in-firmware vs companion), which is a real axis wearing
a backend's name. Renaming it to say so is the fix; flattening it would destroy
information.

```
px4/cpp/uorb/…      ->  px4/cpp/firmware/…     in-firmware PX4 modules, uORB bus, no serialization
px4/rust/xrce/…     ->  px4/rust/companion/…   nano-ros nodes beside PX4; RMW pinned by uxrce_dds_client
zephyr/*/cyclonedds/talker-aemv8r
                    ->  zephyr/*/talker-aemv8r  board variant, per the -aemv8r suffix convention
```

## Ordering principle

**The renames do not depend on the undecided bridge question.** W1–W3 are
mechanical and land first; W4 needs a decision recorded before any code.

## Work items

### W1 — px4: rename the level to its axis — LANDED

**Ordering correction:** the plan had W1.1 rename `cpp/uorb -> cpp/firmware` and
W3.1 move `nros-register-check` out of `examples/`, not noticing that
`cpp/uorb/` contained **only** that module — so W3.1 empties W1.1's target. Doing
W3.1 first means no empty `firmware/` dir gets created for nobody to fill; W4.2
can create it when it has content. Only `rust/xrce -> rust/companion` was a
rename.

- [x] **W1.1** `git mv examples/px4/rust/xrce examples/px4/rust/companion`.
      (`cpp/uorb` dissolved via W3.1 instead — see above.)
- [x] **W1.2** Update the ~29 referencing files: root `Cargo.toml` (exclude
  entries), `just/px4.just`, `scripts/build/compile-check-fixtures.sh`,
  `packages/testing/nros-tests/src/fixtures/binaries/mod.rs`,
  `packages/testing/nros-px4-sitl-test/tests/px4_xrce_e2e.rs`,
  `docs/reference/px4-xrce-companion.md`, `book/src/getting-started/{px4,integration-px4}.md`,
  `integrations/px4/README.md`, `examples/px4/README.md`, `examples/README.md`.
- [x] **W1.3** Delete the px4 structural exemption from `is_allowed()` in
  `scripts/check-example-matrix.sh`, plus the explanatory comment block.

**Acceptance:** `just px4 build-examples` and `just px4 build-fixtures` green;
`check-example-matrix.sh` no longer needs the px4 branch.

### W2 — zephyr: flatten the board variant — LANDED

- [x] **W2.1** `git mv` both `zephyr/{rust,cpp}/cyclonedds/talker-aemv8r` up one
  level to `zephyr/{rust,cpp}/talker-aemv8r`.
- [x] **W2.2** Update the ~28 referencing files: root `Cargo.toml`,
  `just/zephyr-setup.just`,
  `packages/testing/nros-tests/tests/examples_fixture_coverage.rs`,
  `book/src/getting-started/arm-fvp.md`, RFC-0026, and the phase-217 /
  phase-275-276 notes.
- [x] **W2.3** Delete both `allowed_roots` lines.

**Acceptance:** `allowed_roots` is EMPTY and the script still passes; the
aemv8r fixture still builds.

### W3 — the non-example, and the drifted docs — LANDED

- [x] **W3.1** Move `nros-register-check` out of `examples/`. Its own header says
  *"the build itself is the validation"* — it is a link/registration assertion,
  and CLAUDE.md puts non-example binaries under `packages/testing/`. Decide
  between a testing fixture and an `examples/fixtures.toml` build-step
  assertion; either way it stops being an "example".
- [x] **W3.2** Fix `examples/bridges/README.md` (and `examples/templates/README.md`, same drift), which still describes the retired
  `<plat>/<lang>/<rmw>/<example>` form.
- [x] **W3.3** RFC-0026: record that px4's level is a deployment axis, so a future
  reader does not "helpfully" flatten it back.

**Acceptance:** `examples/` contains only examples; no doc describes the
retired path form.

### W4 — uORB interop example + bridge

**uORB carries TWO demonstrations, and they make distinct claims** (maintainer,
2026-07-31). Keeping them distinct is what keeps each honest:

| | claim | proven against | what would falsify it |
| --- | --- | --- | --- |
| **W4.2** direct | nano-ros speaks PX4's in-memory format, so no serialization happens at all | a **stock, unmodified PX4 module** | a stock module cannot read the topic |
| **W4.3** bridge | nano-ros carries uORB traffic out to any RMW it supports, chosen at build time | a **real ROS 2 node** | ROS 2 cannot see the topic, or only one backend works |

Neither is provable by a nano-ros peer at the far end: a nano-ros↔nano-ros test
passes identically whether the encoding is right or wrong, since both sides share
the bug. Foreign peers are the whole measurement.

#### W4.1 — the example's purpose: DECIDED (2026-07-31, maintainer)

> The uORB example demonstrates nano-ros interop with existing PX4 features. The
> writing is different because it skips serialization like ROS does, so other
> upstream PX4 nodes can understand the message format. uORB is the special one
> compared to the others.

That is the thesis, and it is load-bearing in the code rather than an
aspiration. `publisher_publish_raw` checks `len >= meta->o_size` and hands the
caller's bytes straight to `orb_publish`; `publisher_create` ignores
`type_name`, `type_hash`, `qos` and `domain_id` entirely and resolves the topic
to a `const struct orb_metadata *` through `nros_rmw_uorb_register_topic`. So on
this backend:

| | every other backend | uORB |
| --- | --- | --- |
| wire bytes | CDR encoding of the message | the PX4 C struct, verbatim |
| type identity | ROS type name + type hash | `ORB_ID(<topic>)`, a static descriptor |
| serialization cost | encode + decode per sample | none — the payload IS the struct |
| who can read it | another nano-ros / ROS 2 endpoint | **any stock PX4 module**, unmodified |

The last row is the whole point, and it is what no other example in the tree can
show. Everywhere else nano-ros interoperates by speaking a wire protocol; here it
interoperates by sharing PX4's in-memory type. That is also why `uorb/` looked
like an RMW level and was not one — it is not a transport choice, it is the
absence of a transport.

**Consequence for the example:** its message type must come from
`<uORB/topics/*.h>`, not from `nros generate-*`. `publish_raw` /
`subscription_take` are already exposed on both the C and C++ APIs, so this needs
no new machinery — which is the useful finding here: W4.2 is an EXAMPLE, not a
feature.

#### W4.2 — write the interop example

- [ ] A nano-ros node inside a PX4 module that publishes and subscribes a real
      PX4 topic with `publish_raw((const uint8_t *)&msg, sizeof msg)`, registered
      via `nros_rmw_uorb_register_topic("/<topic>", "<ros_type_name>", ORB_ID(<topic>))`.
- [ ] The proof must be a **stock, unmodified** PX4 consumer — e.g. `listener
      <topic>` in the SITL shell, or an upstream module that already subscribes
      it. An assertion that nano-ros can read its own publication proves nothing
      about interop; it is satisfied identically by a correct and a broken
      encoding. (This session hit that exact trap twice — see issue 0351.)
- [ ] Lands at `examples/px4/cpp/firmware/`, which is what creates that dir —
      W3.1 deliberately left it uncreated rather than empty.

**Acceptance:** a reader sees a message crossing between a nano-ros node and an
unmodified PX4 module, with no serialization step on either side, and the test
observes it from the PX4 side.

#### W4.3 — the bridge: DECIDED — uORB → **the build-time-selected RMW** (2026-07-31, maintainer)

Refined from the first answer ("uORB → Zenoh"), and the refinement matters. The
bridge's outward side is **not a fixed backend**. It is the ordinary build-time
RMW knob every other example already uses — cargo `rmw-*` features for Rust,
`-DNROS_RMW=<backend>` for C/C++ — so one bridge at one path builds against
zenoh, xrce or cyclonedds:

```
                    ┌──────────────────────────────┐
  stock PX4 modules │  nano-ros bridge (PX4 module)│  real ROS 2 nodes
   ───── uORB ─────▶│  uORB in   │   RMW out ──────│──────▶
   (no serialization)│           │  -DNROS_RMW=…   │   (zenoh / xrce / cyclonedds)
                    └──────────────────────────────┘
```

This is a better answer than the one I asked for, and it is on this phase's own
thesis: the outward backend is a **build-time choice, not a directory axis**, so
the bridge gets ONE path and no `<rmw>/` level — RFC-0026's rule applied to the
very thing that used to violate it.

It also disposes of the "duplicates `uxrce_dds_client`" objection properly. The
point was never to beat PX4's XRCE client at XRCE. It is that **one** bridge
covers every backend nano-ros supports — including Zenoh, where PX4 has nothing —
and `uxrce_dds_client` covers exactly one, permanently.

**Acceptance: a real ROS 2 node on the far side.** Not a nano-ros peer. This
mirrors W4.2's requirement of a *stock* PX4 consumer, and for the same reason:
each end must be proven against a FOREIGN, unmodified peer, or the demo is
satisfied identically by a correct and a broken encoding.

Honest about what it is not: the serialization uORB avoids returns at the RMW
boundary, necessarily. W4.2 demonstrates the zero-copy property; W4.3
demonstrates reach. Two demos, two distinct claims.

**Sequenced after W4.2**, which establishes the uORB side the bridge reuses.

#### W4.3 scoping — what already works, and what the existing bridges get wrong

Two live backends in one image is **already proven**, which removes the risk I
expected here. `examples/bridges/tt-zenoh-to-cyclonedds` does:

```rust
nros_rmw_zenoh::register().expect("register zenoh backend");
nros_rmw_cyclonedds_sys::register().expect("register cyclonedds backend");
let mut exec = Executor::open_with_rmw("zenoh", &cfg)?;   // then a second session
```

`open_with_rmw` takes the backend by NAME, so build-time selection falls out
naturally: the cargo feature decides which `register()` is compiled in and which
name string is passed. The uORB bridge is the same shape with
`nros_rmw_uorb_register()` inward.

**But the existing bridges encode the backend pair in the DIRECTORY NAME** —
`tt-zenoh-to-cyclonedds`, `tt-zenoh-to-xrce` — with both backends named as hard
crate deps. That is the per-RMW axis this phase just removed from paths,
surviving in a name: two directories that differ only in an outward backend,
which the build could have chosen. The maintainer's design for the uORB bridge
(fixed inward end, build-time outward end, one path) is what these should have
been.

Not decided here, and deliberately not in phase-316's scope: whether
`tt-zenoh-to-*` should collapse to one `tt-zenoh-to-rmw` with the egress
selected at build time. Recorded so the successor phase can weigh it — the uORB
bridge will otherwise be the only one built the right way, and a lone correct
example reads as an inconsistency rather than a rule.

#### W4 blocker (found 2026-07-31): nothing has ever built a nano-ros NODE on uORB

Scoping W4.2 turned up that the uORB backend's proven surface stops below the
node API. Three artifacts look like px4 integration; none of them constructs a
node:

| artifact | what it actually exercises |
| --- | --- |
| `nros-rmw-uorb/tests/register_smoke.cpp` | drives the RMW **vtable directly**, stubbing `nros_rmw_cffi_register` AND the uORB ABI. Never touches `nros-cpp`. |
| `packages/testing/nros-px4-register-check/` | compiles the backend sources inline against real PX4 headers and calls `nros_rmw_uorb_register()`. Proves it LINKS. Does not link `nros-cpp`; the weak `register_fallback.c` is there precisely so it need not. |
| `integrations/px4/module-template/nano_ros_app.cpp` | the node code is a **comment**: *"Replace this comment block with NodeBuilder / Publisher calls"*. |

And there is no `cmake/platform/nano-ros-px4.cmake` — every other platform has
one (`posix`, `zephyr`, `nuttx`, `freertos`, `threadx`, `esp_idf`, `baremetal`);
px4 does not, because no px4 build has ever linked `libnros_cpp.a` /
`libnros_c.a`.

So the honest cost of W4.2 is not "write an example". It is:

1. a px4 platform cmake module, so a `px4_add_module()` can link the Rust
   staticlibs (SITL is a host x86_64 build, so this should be tractable — the
   NuttX board targets are the harder case and are not needed for the demo);
2. the first real node on the uORB backend, which is where any gap between "the
   vtable answers correctly under a mock" and "the API works" will surface;
3. only then the example itself.

That is a phase, not a work item, and it should be split out rather than smuggled
into phase-316's tail. **Recommend: close phase-316 at W1–W3 (done) and open a
successor phase carrying W4.1's decision, W4.2 and W4.3.** The decisions recorded
above are the durable part and travel with it.

This is worth stating plainly because the tree reads otherwise: `examples/README.md`
called `px4/cpp/uorb/nros-register-check` "the canonical PX4 uORB surface", which
is true about LINKING and easy to misread as usage.

## Risks

- **Reference sweep is the whole cost of W1–W2.** ~57 files across `Cargo.toml`
  excludes, just recipes, fixture builders, tests, the book and RFCs. A missed
  reference fails loudly at build time (a path that does not exist), which is
  the good case; the bad case is a stale doc nobody notices — so grep for the
  old paths in `docs/` and `book/` explicitly, not just in code.
- **Fixture rebuild.** Renaming an example directory changes its fixture path;
  per CLAUDE.md any prebuilt fixture keyed on the old path reads stale. Rebuild
  the px4 and zephyr fixture families after W1/W2 rather than debugging a
  "runtime" failure.
- **W4 is not a rename.** It is new example code plus, for the bridge, the
  first non-POSIX entry in `examples/bridges/` (uORB is an in-process bus, so
  the bridge is a C++ PX4 module). Do not scope it with W1–W3.
- **Concurrent sessions.** Other agents are active in this repo; land each W in
  small pushed steps.

## Receipts to collect

| Step | Receipt |
| --- | --- |
| W1 | `just px4 build-examples` + `build-fixtures` green; px4 branch gone from `is_allowed()` |
| W2 | `allowed_roots` empty; `check-example-matrix.sh` passes; aemv8r fixture builds |
| W3 | `examples/` free of non-examples; no doc mentions `<plat>/<lang>/<rmw>/` |
| W4 | interop demo runs against a stock PX4 app; bridge translates uORB → networked backend |

## Receipts collected (2026-07-31)

| Step | Receipt |
| --- | --- |
| W1 | `just px4 build-sitl-cpp` links `libmodules__nros_register_check.a` from the new `EXTERNAL_MODULES_LOCATION` (`nm` shows `nros_rmw_uorb_register`); `just px4 build-fixtures` green from `rust/companion/`; `is_allowed()`'s px4 branch deleted |
| W2 | `allowed_roots` **empty**, `check-example-matrix.sh` passes; `just zephyr build-fvp-aemv8r-cyclonedds` and `-rust` both link `zephyr.elf` from the flattened paths |
| W3 | `examples/` holds no non-examples; the retired `<plat>/<lang>/<rmw>/` form survives only in dated records |
| W4 | not started — blocked on W4.1 |

**Dated records were deliberately NOT rewritten.** `docs/roadmap/archived/`,
`docs/issues/archived/`, `docs/development/audit-findings-*` and the completed
phase-217 / phase-275-276 notes describe the tree as it was on their date;
editing them would falsify a snapshot. Only live docs, code and recipes moved.

## What the acceptance run turned up

Running W2's acceptance is what found the rest of this, and none of it was in
the plan:

- **`examples_canonical_shape` was not failing — it was TIMING OUT** at nextest's
  60 s. Its skip-list prefix-matched `build-` but exact-matched `target`, so it
  walked into all 48 `target-<variant>/` trees a native RMW sweep leaves behind.
  Green on a fresh checkout, red only on a machine that had done the work.
- **Six walkers implemented that same skip rule in five spellings**, two of them
  with real consequences beyond speed: `zephyr::collect_source_files` hashed
  build output into the fixture *signature*, and zephyr's mtime staleness walker
  descended into build dirs whose mtimes are newer than any cutoff by
  construction — it could only ever answer "stale". Converged onto
  `nros_tests::treewalk`.
- **A second presence-vs-truth check in the same test**, masked until the timeout
  was fixed: it flagged `metadata/*.json` by `is_file()` while its own message
  said "must not be TRACKED". Those files are build output and gitignored. Now
  asks `git ls-files`; mutation-tested both ways.
- **Issue 0356**: `px4_e2e.rs` still builds SITL against
  `examples/px4/rust/uorb/{talker,listener}`, retired by phase-277 W7 — whose
  retirement is recorded in a comment 40 lines away in `just/px4.just`. Filed
  rather than fixed: the honest repair is either deleting it or W4.2, and W4.2
  is blocked.

The through-line, and the reason this phase was worth doing: **a rule stated
unconditionally but enforced with exceptions decays into its exceptions**, and a
rule implemented six times decays into six different rules. Both were true here,
one level apart.
