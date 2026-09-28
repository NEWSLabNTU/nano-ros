---
rfc: 0101
title: "Vendored source resolution: one owner per graph, and the hand-off is `links`"
status: Draft
since: 2026-09
last-reviewed: 2026-09-29
implements-tracked-by: [phase-471]
supersedes: []
superseded-by: null
---

# RFC-0101 — Where a vendored source tree comes from

## Summary

A build script that compiles a tree it did not author has to find that tree
first. **Four mechanisms do that in this repository and no document chooses
between them** — the gap phase-471's census measured, widened by one the census
cannot see. This RFC states the rule.

> **At most one crate per resolved dependency graph resolves a given vendored
> tree. Every other crate in that graph reaches it through cargo's `links`
> channel. The one crate that does resolve it resolves through
> `nros_build_paths` — never a raw `env::var`, and never a root derived by
> counting `.parent()` hops.**

`links` is preferred for a reason that is a property of cargo rather than of
taste, and it is measured in [§2](#2-what-links-actually-gives-measured):
it is the only mechanism here that **cargo itself enforces**. A second
resolution through an env variable or a relative literal is invisible until the
two answers disagree; a second `links` owner is a resolve-time error.

**Scope.** Cargo build scripts, the build-script libraries they delegate to
(`nros-board-common`, `nros-zpico-build`, `nros-build-helpers`), and the
RFC-0049 descriptors that carry a source root as data
([§3, D5](#d5--a-envvar-token-in-a-descriptor-is-d3-spelled-declaratively)).
The cmake and west roads resolve sources differently and RFC-0072 owns that
ground — see [§6](#6-what-this-does-not-cover).

## 1. Why this is a new RFC and not an amendment

RFC-0064 says where a **board** declares itself; RFC-0071 says where a
**backend** does. Both were candidates and both are wrong for the same reason:
the subject of this rule is neither. Measured against the tree on 2026-09-29,
the things that resolve a vendored tree are

* **boards** — the three FreeRTOS overlays (kernel, lwIP, `FreeRTOSConfig.h`,
  Tonbandgeraet), `nros-board-threadx` and `nros-board-threadx-linux` (ThreadX,
  NetX Duo), `nros-board-common` (NuttX, ThreadX RISC-V);
* **backends** — `cyclonedds-sys` (Cyclone DDS), `nros-zpico-build`
  (zenoh-pico, mbedTLS), `nros-rmw-xrce-cffi` (micro-XRCE-DDS-Client,
  micro-CDR);
* **an example leaf** — `examples/mps2-an385-baremetal/c/talker`, which is
  neither, and reads `DEP_NROS_C_*` off `nros-c` (issue 1512);
* **a driver** — `packages/drivers/ipc/nvidia-ivc`, whose `NV_SPE_FSP_DIR` is
  the tree's only genuinely out-of-tree SDK root;
* **three RFC-0049 platform descriptors**, which carry nine SDK roots as
  `{env:VAR}` tokens (D5) and are not build scripts at all.

A rule whose subject spans five owners must not live inside one of them, or
each of the others inherits it by cross-reference and can narrow it without
noticing. That is the reach failure the 2026-07-28 audit found in four gates and
issue 0196 names; putting a five-owner rule in a one-owner document is the same
shape one level up, in prose.

RFC-0012 (board/BSP integration) and RFC-0072 (nano-ros is a guest) were also
checked. They own the adjacent question — *who builds the RTOS* — and answer it
for the **host** build (cmake / west / the user's IDE). Neither mentions cargo's
`links` key, `DEP_*` or `CARGO_MANIFEST_DIR`, because the cargo road is not
their subject. RFC-0042 (platform & build determinism) is the nearest in spirit
and its four pillars are headers, capability config, link order and a merge
gate; source-root resolution is none of them.

So: a new RFC, cited by all three, narrowing none of them.

## 2. What `links` actually gives (measured)

The phase-471 brief asserted that `links` "also gives build ORDER, which an env
variable cannot". That is true. Two things beside it are also true and they
bound the preference. All three were measured on 2026-09-29 with a four-crate
throwaway workspace — `owner` with `links = "expt"`, `plain` without, `consumer`
depending on both, `grand` depending on `consumer` — each build script printing
a wall-clock stamp.

### 2.1 It gives build-script ORDER; a plain dependency does not

`owner` slept 2 s, `plain` slept 10 s, both started together:

```
owner    END   1790633928966
consumer START 1790633928971      <- 5 ms after the links dep finished
plain    END   1790633936962      <- 7 991 ms AFTER the consumer already ran
```

A dependent's build script is ordered after a `links` dependency's build script
and is **not** ordered after a plain one. So a value a sibling build script
merely *sets in the environment* cannot be read at all: nothing says the sibling
has run. This is why `examples/mps2-an385-baremetal/c/talker` takes the channel
rather than a path — its own comment: *"`links` is also what makes cargo run
THIS script after the one that writes the config header, instead of racing it."*

### 2.2 The channel is exactly ONE HOP — order and metadata both

`grand` depends on `consumer` depends on `owner`:

```
owner  START 1790633952402
grand  START 1790633952406        <- concurrent with owner, not after it
grand  DEP_EXPT_ROOT=Err(NotPresent)
owner  END   1790633954403
consumer START 1790633954412      <- ordered, and Ok("/from-owner")
```

`DEP_*` reaches immediate dependents only, and so does the ordering. A value
that must travel two hops is **re-published** by the middle crate under its own
`links` key. It does not fall through, and a build script that assumes it does
gets `Err(NotPresent)` rather than a diagnostic.

### 2.3 Cargo enforces the single owner

Giving `plain` the same `links = "expt"`:

```
error: Attempting to resolve a dependency with more than one crate with links=expt.
```

This is the load-bearing property. Duplicate resolution of one thing is the
failure mode this repository has paid for repeatedly in adjacent form — two
cargo workspace roots sharing a `--target-dir` (issue 0616), two Corrosion
copies sharing `cargo/build` (issue 0500), two hand-copied source lists for one
XRCE archive (issue 1068), two derivations of one fixture artifact dir
(issue 1025). In every one of those both answers were *computed correctly* and
differed, so every gate stayed green. `links` is the only mechanism here that
turns that class into a resolve-time error instead of a convention.

### 2.4 The two limits, both already measured in-tree

* **`links` reaches cargo dependents only.** A cmake or west consumer reads no
  `DEP_<LINKS>_*` at all. `nros-rmw-xrce-cffi/build.rs` records the
  measurement (2026-09-05): *"a `links` key buys nothing here because the
  consumer is a CMake project, which can read no `DEP_<LINKS>_*` at all"*, and
  publishes an `OUT_DIR` pointer file instead. That is the sanctioned fallback,
  not a workaround — see D2's exception.
* **A `links` key is a claim in a global namespace.** Because cargo permits one
  per graph, declaring it commits every future consumer. Name it after the
  native tree as its own ecosystem knows it (`ddsc`, `zpico`, `nros_c`), never
  after the declaring crate's role.

## 3. Design

### D1 — one owner per resolved graph

At most one crate in a resolved dependency graph resolves the root of a given
vendored tree. **"Per graph", not "per repository"**: the three FreeRTOS
overlays each call `nros_build_paths::freertos_dir()` and that is *correct*,
because exactly one board is active per image (RFC-0072 §3, measured), so no two
of them are ever in one graph. It is also exactly the scope cargo's own `links`
rule uses — §2.3's error is raised at dependency resolution, over the resolved
graph.

### D2 — the hand-off is `links`, and the value is republished, never re-derived

A crate that needs a tree another crate **in its own graph** already resolved
reads `DEP_<LINKS>_<KEY>`. It does not resolve the tree a second time and does
not reach for the env variable the owner used.

If the owner has no `links` key, **give it one.** That is a one-line manifest
change plus `cargo:<key>=` prints; the alternative is a second resolution, which
§2.3 is the argument against.

Two hops means two publications (§2.2): the middle crate declares its own
`links` and re-prints what it must forward.

**The converse holds too: do not declare a `links` key you do not publish on.**
Of the eleven `links` declarations in the tree, three carry a live channel
(`ddsc` → two paths, `nros_c` → three paths, `nros_node` → three sizes) and
**eight are unread** — the six generated message crates' `bounds_json`, the
`cyclonedds` crate's `present=1`, and `zpico`, which emits no
`cargo:<key>=` metadata at all. An unread key still reserves a global name and
still imposes §2.3's uniqueness on every future graph, for nothing.

**Exception — a non-cargo consumer.** Where the consumer is a cmake or west
build, `links` is unavailable by construction (§2.4). The owner then publishes
into `OUT_DIR` a file the consumer reads, and the consumer locates `OUT_DIR`
from cargo's own `build-script-executed` JSON — **never** by globbing
`target/*/build/<crate>-*/out`, because taking the first match of a glob is
issue 0500's defect. `nros-rmw-xrce-cffi`'s `nros-xrce-vendor-build.txt` is the
worked example, and `just check rmw-xrce` is the consumer that reads it.

### D3 — the owner resolves through `nros_build_paths`

The crate that does own the resolution uses exactly one of two calls, and the
choice is a property of **the tree**, not of the crate:

| can this repository ship a copy of the tree? | call | when the variable is unset |
| --- | --- | --- |
| yes — a submodule, or a directory we author | `nros_build_paths::env_or_repo_path("<VAR>", "<repo-relative>")` | the in-repo path, canonicalised |
| no — the tree is the user's SDK and we ship nothing | `nros_build_paths::env_path("<VAR>")` | `None`, and the caller fails with a NAMED remedy — never a guess |

Both arms apply issue 1280's three-valued rule (outside any checkout → keep; a
different checkout → re-root onto this one; this one → keep) and both
canonicalise, which is what keeps issue 0491 from firing on the three spellings
one directory has here. A raw `env::var` of a path-valued name does neither.
This is phase-471 W3's rule and its gate; D3 states it where a new crate will
read it.

**A private helper is the hazard, not the fix.** A local
`fn env_path(name: &str)` takes the variable name as an *argument*, so no
literal-matching probe can see which variables it resolves — which is how issue
1527's five sites drifted with every gate green. Two such helpers were converted
to delegate to the shared resolver; a third, `env_path_or` in
`nros-board-common/src/threadx_qemu_riscv64_build.rs`, is still
`env::var(name).unwrap_or(default)` one file away from its converted twin, whose
doc comment warns about exactly this. It is invisible for a second reason: it is
not in a `build.rs`, and the census reads `build.rs` files. Issue 1560.

**A repo-relative default is `repo_root()`, never a counted walk.** Deriving the
repository root by chaining `.parent()` or joining `"../../.."` is forbidden.
`nros_build_paths::repo_root()` walks up for the `nros-sdk-index.toml` marker
and is therefore depth-independent; a counted walk encodes the crate's current
depth, and this tree has **three recorded defects** from exactly that:

* `nros-rmw-xrce-cffi/build.rs` — when the crate moved from `packages/xrce/` to
  `packages/rmw/xrce/`, three parents became four and in between *"every
  vendored path came out doubled"*. Its own comment states why a rule is needed:
  *"A `.parent()` chain is a relative path that no grep for `../` can find —
  only a build does."*
* `nros-board-*-freertos/build.rs` (three copies) — issue 0365: the walk to the
  `nros-c` headers *"was left at the old `core/nros-c`, so the TU could not find
  the header."*
* `nros-zpico-build/src/runner.rs` — `manifest_dir.join("../../../../config")`
  kept resolving after the platform descriptors moved out of that directory, so
  *"every platform silently fell back to builtins. A wrong image, no
  diagnostic."*

Every one of them was fixed by re-counting the hops, which is why the class
survived. Issue 1558.

A CWD-relative literal (`"../../core/nros-rmw-abi/include"`, as in
`packages/rmw/cffi/build.rs` and `nros-platform-cffi/build.rs`) is the same
rule's third spelling: it happens to work because a build script's cwd is its
crate root, and it moves when the crate does.

### D4 — the `links` key names the tree; the env variable names the SDK row

A corollary that decides the key and the variable name together. The `links`
value names the native tree as its own ecosystem knows it, because it is a claim
over the whole graph (D2). The env variable names the same tree in the
vocabulary of `just/sdk-env.just`, because that file is the shell half of the
same subject and issue 1280's gate reads its variable set from there.

**A path-valued variable a build input honours that `sdk-env.just` does not
export is outside that gate's reach.** The rule is about path-valued build
inputs; the gate's subject is that file's rows; where the two differ the gate
reports green about a variable it never saw. `ZENOH_PICO_DIR` and
`NV_SPE_FSP_DIR` are in that gap today. Issue 1560.

### D5 — a `{env:VAR}` token in a descriptor is D3 spelled declaratively

Nine SDK roots — `FREERTOS_DIR`, `FREERTOS_PORT`, `FREERTOS_CONFIG_DIR`,
`LWIP_DIR`, `THREADX_DIR`, `THREADX_CONFIG_DIR`, `NETX_DIR`,
`NETX_CONFIG_DIR`, `NUTTX_DIR` — reach the zenoh-pico compile not from Rust at
all but as `{env:VAR}` tokens in three RFC-0049 platform descriptors
(`packages/platform/nros-platform-{freertos,threadx,nuttx}/nros-platform.toml`),
interpolated by `nros-platform-config`. **A census of build scripts cannot see
them**, which is the second reason this RFC exists rather than a comment in one.

This is the right shape and it stays: it is RFC-0049's platform rung, the
manifest's `required_env` validates presence, and a row can be
capability-gated (`when = { capability = { ip_stack = true } }`, issue 1143) in
a way no `if` in a build script reads as well. What it must adopt is D3's
*resolver*: the interpolator's `env:` arm is a bare `std::env::var`, so none of
the nine carries the re-root rule. The precedent is in the same `match`: the
`{nuttx_include}` arm already routes through `nros_build_paths`, with a comment
saying the shared spelling is the point. Issue 1560.

**A descriptor token is not an exemption from D1.** The token and a build
script's `env_or_repo_path` of the same variable are the *same* resolution
expressed twice, and they disagree on the in-repo default: the token has none,
so an unset variable is an `InterpError::MissingEnv` where the Rust arm silently
succeeds against the submodule. Prefer whichever one the consumer can read, and
do not write both for one tree in one graph.

## 4. The test that decides which case a new crate is in

Ordered; first match wins.

1. **Am I resolving a foreign root at all?** `OUT_DIR`, my own `src/`, my own
   `c/`, a file codegen wrote — no. Nothing here applies.
2. **Does a crate already in my graph resolve this tree?**
   Check with `git grep -n '^links *=' -- '*/Cargo.toml'` and the SOURCE ORIGIN
   section of `python3 scripts/nros-build-wiring.py --scripts`.
   * **Yes, and it declares `links`** → read `DEP_<LINKS>_<KEY>`; add the
     dependency if it is not one already. **Done.**
   * **Yes, and it does not** → give it one (D2). **Done.**
   * **Yes, but it is two hops away** → the middle crate republishes (§2.2).
   * **Yes, but I am not a cargo crate** → the `OUT_DIR` pointer file (D2's
     exception).
   * **No — the owner is not in my graph at all** (the three FreeRTOS boards
     reaching the `nros-c` headers with no `nros-c` dependency, today) → you are
     the owner for your graph; go to 3.
3. **I am the owner. Can this repository ship a copy of this tree?**
   * **Yes** (a submodule, or a directory we author) →
     `env_or_repo_path("<VAR>", "<repo-relative>")`, and add the row to
     `just/sdk-env.just` so the shell half re-roots the same variable (D4).
   * **No** (the user's Cube / MCUXpresso / vendor tree; we ship nothing) →
     `env_path("<VAR>")` with a named failure on `None`.
   * **And if the consumer of the root is a descriptor rather than code** →
     a `{env:VAR}` row (D5), same variable, same resolver.

Step 3's question is about the TREE and has an objective answer: *is there a
copy in this repository's `.gitmodules`, or could there be?* All sixteen named
resolvers in `nros_build_paths` answer yes — every one of them is
`env_or_repo_path` — which is why the "no" arm reads as theoretical. It is not:
it is the arm RFC-0072's real user (a
vendored STM32Cube FreeRTOS) lands on, and the one in-tree crate that takes it
today is `nvidia-ivc`, whose FSP ships under an SDK-Manager EULA and can never
be vendored. Writing it down is what keeps the next one from inventing a fifth
shape.

## 5. The tree, against the rule (measured 2026-09-29)

`python3 scripts/nros-build-wiring.py --scripts` reports the roles and its
three `source_origin` values; the rows below add the owner/consumer split and
the descriptor road, neither of which the census models.

| tree | owner | mechanism | conforms |
| --- | --- | --- | --- |
| FreeRTOS kernel, lwIP | the three `nros-board-*-freertos` overlays | `env_or_repo_path` via `freertos_dir()` / `lwip_dir()` | yes (D1 — one per graph) |
| `FreeRTOSConfig.h` dir | same three | `env_path("FREERTOS_CONFIG_DIR")` + a crate-local default | yes |
| Tonbandgeraet | `nros-board-mps2-an385-freertos` | `env_or_repo_path` via `tband_dir()` | yes |
| ThreadX, NetX Duo | `nros-board-threadx`, `nros-board-threadx-linux` | `env_path` / a delegating `env_path_or` | yes |
| ThreadX, NetX Duo (RISC-V) | `nros-board-common::threadx_qemu_riscv64_build` | a private `env_path_or` that is a RAW `env::var` | **no** — issue 1560 |
| NuttX | `nros-board-common` | `nuttx_dir()`, plus three raw `NUTTX_DIR` reads | open — phase-471 W6 |
| Cyclone DDS | `cyclonedds-sys` | `env_or_repo_path("CYCLONEDDS_SOURCE_DIR", …)`, published as `DEP_DDSC_*` | yes — the exemplar |
| Cyclone DDS (consumer) | `nros-rmw-cyclonedds-sys` | `links` hand-off | yes — the exemplar |
| `nros-c` headers (consumer) | `examples/mps2-an385-baremetal/c/talker` | `links` hand-off (`DEP_NROS_C_*`) | yes (issue 1512) |
| `nros-c` headers (boards) | three FreeRTOS overlays + two ThreadX | hop-counted `.parent()` walk | **no** — issue 1558 |
| micro-XRCE-DDS-Client, micro-CDR | `nros-rmw-xrce-cffi` | four-`.parent()` walk, then a literal; no override | **no** — issue 1558 |
| zenoh-pico, mbedTLS | `nros-zpico-build` | `CARGO_MANIFEST_DIR`-relative; the override is a raw `env::var("ZENOH_PICO_DIR")` | **partly** — issues 1558, 1560 |
| the nine SDK roots the zpico compile uses | three `nros-platform.toml` descriptors | `{env:VAR}`, interpolated by a bare `std::env::var` | **partly** — D5, issue 1560 |
| NVIDIA Orin SPE FSP | `packages/drivers/ipc/nvidia-ivc` | raw `env::var("NV_SPE_FSP_DIR")`, feature-gated | **no** — issue 1560 (D3's "no" arm, done raw) |

Three notes on what the non-conforming rows are **not**:

* They are not D2 failures. Each is the sole owner of its tree in its own graph
  — no board crate depends on `nros-c`, so `DEP_NROS_C_*` is genuinely
  unreachable from one. What they get wrong is D3's *how*, not D1's *who*.
* zenoh-pico's `CARGO_MANIFEST_DIR`-relative default is not itself a defect: the
  submodule is checked out **inside** the crate
  (`packages/rmw/zenoh/zpico-sys/zenoh-pico/`), so crate-relative and
  repo-relative name the same directory and the crate-relative spelling is the
  one that survives the crate moving. What is wrong there is the separate
  `repo_root` hop count and the raw override read.
* `nvidia-ivc` is the one row where the raw read is nearly harmless — an SDK
  under an EULA is by construction outside every checkout, which is the case
  `reroot_foreign` deliberately leaves alone. It is listed because D3 is about
  the *call*, not about which value happens to arrive; a rule with a "when it
  would not have mattered" arm is not checkable.

**Named exceptions.** The 12 byte-identical Zephyr leaf `build.rs` shims resolve
nothing and are out of scope — and would stay out of scope if they did: a
copy-out leaf (RFC-0026) may not depend on a crate that exists only in this
checkout, so a leaf that ever needed a tree root would take D3's env-variable
arm with no in-repo default. The same protection covers
`packages/reference/stm32f4-porting/*`, which its README calls *"templates for
BSP developers"*.

## 6. What this does not cover

* **The cmake and west roads.** A cmake consumer resolves Cyclone through
  `just/cyclonedds.just` and the SDK store; a west build resolves modules
  through `west.yml`. Those are RFC-0072's and RFC-0085's ground, and
  [canonical-build-path.md](../reference/canonical-build-path.md) is where the
  three roads meet. A tree resolved on two roads is resolved twice by
  construction; that is a known cost of having three roads, not something D1
  can forbid.
* **Which version of a tree to pin.** A separate decision with its own
  precedent — issue 0507 for Cyclone, issue 0609 and RFC-0075 for zenoh. The pin
  is an interop decision; this RFC only says how a build script *finds* whatever
  was pinned.
* **A prebuilt install prefix.** `ZENOH_PICO_DIR` under the `system-zenohpico`
  feature names an install prefix, not a source tree, so there is no root to
  resolve. It appears in §5 because it is read raw, which D3 forbids whatever
  the path names.

## 7. Alternatives considered

* **Amend RFC-0064 or RFC-0071.** Rejected in §1: the subject spans boards,
  backends, a driver, an example leaf and three descriptors.
* **Prefer the env variable everywhere, for uniformity.** Rejected by §2.3 —
  uniformity here buys a mechanism that cannot detect its own duplication — and
  by §2.1, which shows it cannot even be read reliably between sibling build
  scripts.
* **Prefer `links` everywhere, including for the owner.** Not expressible:
  `links` is how a *second* crate reaches a tree a *first* one already resolved,
  so the owner still has to resolve it somehow. The phase-471 brief's flat
  three-way preference order reads as if the three were alternatives at one
  site; they are not, and D1/D3 split the two questions apart.
* **A `nros-vendored-sources` crate resolving every tree centrally.** Rejected
  by issue 1208's lesson — a shared home is only right when the things sharing
  it answer the same question, and a FreeRTOS kernel root and a Cyclone source
  dir do not. `nros-build-paths` already holds the *resolution primitive*
  without holding the *decision* of who calls it, which is the correct split.
* **Ban the `{env:VAR}` descriptor tokens and move those nine roots into Rust.**
  Rejected by D5: the declarative form is the only one that can carry a
  capability gate, and moving it would trade a reach problem for a second
  authority over the same fact.

## 8. Open questions

1. **Should an override that lands on a pinned-for-interop tree announce
   itself?** `CYCLONEDDS_SOURCE_DIR` silently replaces a pin that is an interop
   decision (issue 0507). Issue 0500's lesson — read the printed line, never
   infer — argues for printing the origin. Not decided here, because nothing in
   the tree does it and an undeclared decision is a claim.
2. **How far should D3's gate reach?** Issue 1560 collects four gaps of one
   shape (a shared build crate that is not a `build.rs`, a descriptor token, and
   two variables with no `sdk-env.just` row). Whether the fix is to widen the
   gate's subject to "path-valued build input" or to require every such input to
   have an `sdk-env.just` row is open, and the two answers differ for
   `NV_SPE_FSP_DIR`.
3. **Should the eight unread `links` keys be removed?** D2 says do not declare
   one you do not publish on; six of the eight are generated message crates,
   where the key is emitted by codegen and removing it is a codegen change
   rather than eight edits.

## Changelog

- 2026-09 — created (phase-471 W5). `links` ordering, one-hop scope and
  single-owner enforcement measured rather than asserted; the tree checked
  against the rule and the non-conformances filed as issues 1558 and 1560.
