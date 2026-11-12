# Phase-555 W2: build-toolchain-codegen review, verdicts and decision brief

Reviewer: nros 241, 2026-11-13. This covers the 27 RFCs in `area: build-toolchain-codegen`.
The mechanical fixes are in this PR, and every one of the 27 has `last-reviewed: 2026-11`.
RFC-0030 moves to `area: rmw-wire` (it is a Cyclone dynamic-type-builder proposal), so 26
stay in this area. No better area was found for the rest; 0085 (west road) and
0119 (CLI extension kinds) were the candidates, and both are about the build.

Each verdict below is the reviewer's recommendation. A verdict that changes a status or a
design waits for the ruling on its item in the brief.

RFC-0130 (one ROS-CMake build, `amends: [0065, 0024]`) is in PR #3634, which is merging now.
It is not in this tree; the review read it from that branch. Items that depend on it say so.

## Verdicts

| RFC | Title (short) | Verdict | Evidence |
|---|---|---|---|
| 0014 | `nros setup` | amend (B15) | Index/store/lock model holds. §1's `[tool.zenohd]` and the `zenohd` space rows are false: the index marks `zenohd` removed (`nros-sdk-index.toml:450-452`, RFC-0075). 0062/0090/0095/0099 amend it with no pointer back. |
| 0023 | Codegen workspace / ament discovery | amend (B15) | Stages A/B landed (`rosidl_generate_interfaces` NanoRosGenerateInterfaces.cmake:1041, `nros_workspace_interfaces` :1128). Stage C's `--workspace` verb and Stage D's `nros-build-codegen` crate were never built; Stage E shipped as `nros dev ws env` (dev.rs:182). |
| 0030 | Sequence-of-nested (Phase 212.K.7.4.c) | **archive** (B5) | Path A landed in `705e1b245`; `DDS_OP_SUBTYPE_STU` in `bridge/dynamic_type_builder.cpp`, tests `registry_seq_nested.rs`. No tracking phase; baselined. Area moved to `rmw-wire`. |
| 0032 | Entry-codegen pipeline | **amend, then Stable** (B6) | The banner says a committed `system_model.yaml` + `model =`; now `launch =` is the input and `model =` is deprecated (main_macro.rs:513,590). §2's emitters are RFC-0091's packs. §4-§7's sched-task contract is live (`run_sched_tasks`, `SchedTaskSpec`). |
| 0033 | Per-field capacity config | keep | Every named item exists: `CapacityResolver` (cargo-nano-ros lib.rs:401), `_nros_codegen_config_chain` (NanoRosCodegenCore.cmake:494-507), `--codegen-config` (rosidl-lower config.rs:573). |
| 0040 | Distribution model, `nros new` deps | **amend** (B7, B9) | D1 (no crates.io) holds (scaffold.rs:659-669). D2's patch home is RFC-0098's generated config; D3's checkout path meets RFC-0095; D5's "no build/run" is false (`nros build`, `nros run`, cli.md:423,685). |
| 0042 | Platform & build determinism | keep (B16) | Stable and landed; `nros-platform-api` single header, `NanoRosCapabilities.cmake`, `staticlib_duplicate_symbols` (`just check staticlib-symbols`). D4's gate was renamed to `platform_header_compile.rs` (fixed). |
| 0048 | Ament-aligned CMake consumption | **supersede-by 0130** (B1) | §4's `<nano_ros deploy board rmw/>` tuple is a FATAL_ERROR now (NanoRosPackageXml.cmake:182-187). §1's "activate.sh exports `nano_ros_ROOT`" is false (activate.sh:57-63). |
| 0054 | C headers as ABI SSoT | amend (B15) | "Layout tests ON" was reversed: `--no-layout-tests` on all surfaces (gen-abi-bindings.sh:10-11,275,313,342). Three surfaces now (`board.h`), not two. |
| 0062 | One dependency SSoT | keep | Stable: 52 `[prereq.*]` rows, 0 `[system.*]`; `nros-rosdep-snapshot.toml` + `rosdep_snapshot.rs` (Amendment 5); gate `dist-runtime-deps`. |
| 0065 | Colcon-like builder | **amend D3, then Stable** (B2, B4) | D3 is amended by RFC-0130 for Rust-startup boards; its "rule that covers every exception" names two exceptions that no longer hold (RFC-0085 D4, RFC-0121 D6). Code is live (`plan.rs` `driver_for_board`/`NoRoad`, `IdfPy` gone). |
| 0067 | Msg-dep identity | keep, **Draft → Stable** (B8) | D1-D5 landed: gates `msg-dep-is-path` (codegen.just:685) and `leaf-lockfiles`; phase-465 "LANDED -- all five waves". |
| 0068 | Codegen IR | keep (B14) | Four stage crates exist; Amendments 1-4 current. OQ1 (minijinja) and OQ3 (`NROS_TEMPLATE_DIR`, render.rs:117-121) are answered; `nros dev codegen export` was never built. |
| 0070 | Build caches | keep, **Draft → Stable** (B8, B15) | R1/R3/R4 paths verified (`checkout-paths.sh`, `NanoRosCheckoutRoot.cmake`, `build_root.py`); gate `example-leaf-target-dirs` (fixtures.just:117); five tracking phases archived. R1 still lists `zenohd`. |
| 0085 | Zephyr workspace and west | **amend D4** (B13) | D4 teaches a node package naming its board (`DEPLOY zephyr`), against RFC-0121 D1 and RFC-0130 D1; `examples/workspaces/rust/src/zephyr_entry` deleted (`50f50be333`). |
| 0087 | Package identity, provider format | keep, **Draft → Stable** (B8, B14) | phase-420 complete; `provider_scan`, `package_paths`, `--packages-select/-up-to` (cmd/build.rs:329,355), four gates present. Its open question was settled by the code (NanoRosPackageXml.cmake:182-186). |
| 0090 | Codegen version token | keep, **Draft → Stable** (B8, B14) | `abi_guard.rs`, `stale_guard.rs`, `codegen_version.rs` present; phase-429 complete. OQ2 answered (NanoRosCodegenCore.cmake:250-294 reads `codegen_version.rs`). |
| 0091 | One producer, many packs | keep | Stable; `nros-entry-lower`, `nros dev codegen entry`/`entry-node`/`entry-pack` live; §7's six `.cpp.in` deleted. Consistent with RFC-0130 D2/D4. |
| 0094 | Resolve before configure | keep | Stable, phase-439 complete. `resolved.toml` (NanoRosResolved.cmake), `nros_cli_core::routing`, gate `package-routing` live; RFC-0120 A3 banner present. |
| 0095 | The store is the root | keep, **Draft → Stable** (B8, B12) | phase-440 complete. `nros-toolchain.toml`, `store::discover_pin_files`, `toolchains/<ver>` (cmd/pin.rs:284), `nros-launcher` live. |
| 0097 | Release composition | keep, **Draft → Stable** (B8) | phase-443 and phase-541 done. D4 is `packages/cli/nros-launcher`; D7's `share/nros/manifest.toml` written by release-nros.yml:239. D2/D3 are "not scheduled". |
| 0098 | Generated leaf build config | **amend** (B3) | Current through RFC-0121's banners, but ~508 and ~660 still treat "bare-metal C/C++ apps on a cargo link root" as live, which RFC-0130 retires. Named gates and `drops_ip_link` live. |
| 0099 | Provisioning planned once | keep, **Draft → Stable** (B8, B12) | phase-447 and phase-517 complete. `ensure_tools`, `front_newest`, `SessionRun`, gates `dist-floors`, `zephyr-module-allowlist` live. |
| 0101 | Vendored source resolution | keep, **Draft → Stable** (B3, B8) | phase-471 all landed; issues 1558/1560 resolved 2026-09-29; `nros_build_paths` API exists and boards use `locate::source`. §5 names a cargo-built C leaf as the exemplar consumer. |
| 0104 | Pinning and project lock | **amend D9** (B11) | Stable, phase-485 archived; `locks/cargo-registry.lock`, `nros update`, `drift.rs` live, `scripts/bin/cargo` gone. D9 says it replaces `check-leaf-lockfiles`, which still runs (cargo.just:463). |
| 0119 | CLI extension packages | **amend** (B9, B10) | `packages/runners/` holds `nros-extension` + 7 runners. D5 publishes `nros-extension` to crates.io, against RFC-0040 D1; D4's root-`system.toml` fallback is retired by RFC-0121 with no pointer back. |
| 0120 | `nros configure` | keep | Draft, phase-530 open. `nros configure` + `sync` alias (cmd/user.rs), `nros_image_kconfig_hook` live, `NanoRosReconfigure.cmake` deleted as D5 says. Matches the 2026-11-12 NuttX ruling. |

Mechanical fixes made in this PR:

- **Dead and moved paths:**
  - 0014: `aarch64/cortex-r-rust-patch.sh` → `scripts/zephyr/cortex-r-rust-patch.sh`; the
    phase-172 see-also → `archived/`.
  - 0032: `codegen/entry/emit_{rust,cpp,c}.rs` → `emit_rust.rs` + `packs/entry/{rust,cpp,c}`;
    `phase228_sched_task_filter.rs` → `sched_task_filter.rs` (two sites); `nros/main.hpp` →
    `nros-cpp/include/rclcpp/detail/main.hpp`; phase-212, phase-228 and issue 0045 → `archived/`.
  - 0042: D4's gate → `platform_header_compile.rs` (`just check source-gates`) and
    `just check staticlib-symbols`.
  - 0048: `./nano-ros/bootstrap.sh` → `./nano-ros/scripts/bootstrap.sh`.
  - 0094: `NanoRosNodeRegister.cmake:1229` → `_nros_entities_retired` (:522).
  - 0085: the deleted `zephyr_entry` path annotated "since deleted, generated per image".
  - 0068: undefined `[RFC-0056]` / `[RFC-0054]` reference links → relative links.
- **Retired names:**
  - 0014: board `native` → `linux`; `store gc` → `toolchain gc`.
  - 0032: `(native/posix -> posix` → `(linux -> posix`.
  - 0087: D6's `nros ws packages` → `nros dev ws providers`.
  - 0095: the retired `ros2-box-sync.sh` named as "(since retired, D12)".
- **Missing `amends:` links**, each declared in the RFC's own prose:
  - 0042 → 0006, 0031 (also added by the rmw-wire PR #3659; the later rebase resolves to one line);
  - 0062 → 0014;
  - 0065 → 0024, 0026, 0048, 0070;
  - 0067 → 0023, 0026, 0048;
  - 0085 → 0065;
  - 0090 → 0014, 0023, 0065, 0068;
  - 0091 → 0068;
  - 0094 → 0065, 0071, 0087 (also in #3659);
  - 0095 → 0014;
  - 0097 → 0090;
  - 0098 → 0032, 0049, 0065;
  - 0099 → 0014, 0062;
  - 0104 → 0065.
- **"Amended by" lines:** 0065 gains one header line naming 0085, 0090, 0094, 0098, 0104 and
  RFC-0130 (plain text until it lands); 0068's existing line gains 0090.
- **Dated notes:**
  - 0065 D3: west apps are generated per image (RFC-0085 D4) and RFC-0121 D6 retires the
    plain-cargo copy-out flow; D14's `scripts/bin/cargo` row notes the shim is retired (RFC-0104 D9).
  - 0090 OQ3: the ESP-IDF half is moot (integration deleted, phase-468 W2).
  - 0099: the cargo shim is retired; `hal_espressif` dropped 2026-09-27; D8 amended by RFC-0118.
  - 0101: changelog line recording the 1558/1560 resolution.
  - 0023: Stage E's book page "(landed as `book/src/tutorial/your-own-message.md`)".
- **Area:** 0030 `build-toolchain-codegen` → `rmw-wire`.
- **`last-reviewed: 2026-11`** on all 27: 17 set in this PR, 10 already read 2026-11.

`python3 scripts/check-rfc-phase-tracking.py` passes after each batch: 119 pass, 8 baselined.

## Decision brief

Each item gives the measured facts, the options and a recommendation.

### RFC-0130 follow-through (depends on PR #3634)

**B1. RFC-0048: supersede by RFC-0130.**

- **Facts:** two of 0048's six design sections are already reversed: §4's per-package
  `deploy board rmw` tuple is refused (NanoRosPackageXml.cmake:182-187; board and rmw moved to
  the leaf `system.toml`, RFC-0098 D3/D5), and §1's root export is gone (activate.sh:57-63,
  RFC-0103 D6). The verbs and presets survive (NanoRosVerbs.cmake:127,434; setup.rs:219).
  RFC-0130 makes a C/C++ node package an ordinary ROS 2 CMake package on every board, which
  is 0048's goal carried further. Phase-287 is archived; 0048 is still Draft.
- **Options:** (a) when 0130 lands, add `supersedes: [0048]` to it and move 0048 to
  `archived/` with a forward pointer; (b) keep 0048 as the reference for `find_package` and
  the verbs, with 0130 amending it.
- **Recommendation:** (a). Until then, no banner is needed beyond what this PR records.

**B2. RFC-0065 D3: rewrite to one current table, then Stable.**

- **Facts:** D3 has three layers already (2026-08-26 correction, 2026-09-10 supersession by
  RFC-0098 D9, 2026-09-28 `IdfPy` deletion), RFC-0130 extends its cmake row to Rust-startup
  boards, and its exceptions paragraph names two exceptions that are gone (west apps are
  generated per image, RFC-0085 D4; plain-cargo copy-out retired, RFC-0121 D6). Phase-383 is
  archived complete and nine RFCs build on 0065.
- **Options:** (a) after 0130 lands, rewrite D3 as one table (cargo: pure Rust; cmake: any
  C/C++ package on any board, CMake owning the link; west: Zephyr; refusal: esp32
  cross-language), move the superseded paragraphs to the changelog, flip to Stable;
  (b) keep layering dated notes, stay Draft.
- **Recommendation:** (a), once phase-553 lands the road.

**B3. RFC-0098 and RFC-0101 still describe cargo-built bare-metal C/C++ leaves.**

- **Facts:** 0098 ~508 ("the mps2 bare-metal C/C++ apps on a cargo link root") and ~660
  (managed `<depend>` rows skip "a C/C++ application with a Rust link root"); 0101 §5 names
  `examples/mps2-an385-baremetal/c/talker` as the `DEP_NROS_C_*` exemplar consumer, and
  RFC-0103:461 leans on `DEP_NROS_C_*` too. RFC-0130 retires the cargo-crate bare-metal C/C++
  workspaces as a later migration; six such leaves exist under
  `examples/mps2-an385-baremetal/c/` today.
- **Options:** (a) at 0130's merge, banner both passages "Amended by RFC-0130" and add 0098,
  0101 to 0130's `amends:`; (b) wait until phase-553 deletes the leaves, then strike the text.
- **Recommendation:** (a), with the strike-through as a phase-553 work item. The passages are
  guidance someone could still follow.

**B4. Copy-out examples: is "builds with plain cargo/cmake and no `nros`" still a promise?**

- **Facts:** 0065 D3 and 0070 R1's copy-out row both justify an exception with it. RFC-0098 D2
  already says a copy-out example builds "after `nros configure`", `examples/**/.cargo/` is
  empty (gate `example-cargo-dirs`), and RFC-0121 D6 (lines 222-226) retires the plain
  `cargo build --manifest-path <leaf>` flow. CLAUDE.md still calls examples "standalone
  copy-out projects".
- **Options:** (a) drop the promise everywhere: a copy-out project keeps native output dirs
  beside its source and builds through `nros build` / `nros configure`; (b) keep it for C/C++
  only (under RFC-0130 an ordinary ROS CMake package), drop it for Rust, which needs generated
  config (RFC-0098).
- **Recommendation:** (b), ruled once, with 0065 D3 and 0070 R1 pointing at the ruling.

### Supersede, archive or rewrite

**B5. RFC-0030: archive.**

- **Facts:** the title is a phase-item id. Path A landed (`705e1b245`, `SUBTYPE_STU`, tests
  present); Path B's `idl/` directory was never needed. Only README/ARCHITECTURE cite it. It
  has no tracking phase and is in `.config/rfc-phase-tracking-baseline.txt`.
- **Options:** (a) move to `docs/design/archived/` and drop its baseline line; (b) keep it live
  under `rmw-wire` with `implements-tracked-by: [phase-212]` and a back-reference in the
  archived phase-212 doc (K.7.4.c row, line 801).
- **Recommendation:** (a). It now sits in `rmw-wire`, whose brief does not list it, so the
  ruling belongs here.

**B6. RFC-0032: amend in place, or split?**

- **Facts:** 0032 holds three things: the emitter inventory (§2, now RFC-0091's packs;
  `emit_c.rs`/`emit_cpp.rs` deleted, RFC-0091:570), the boot scaffolds (§3, whose
  `nros::main!(board = X)` is retired in favour of `[image.<id>] board`, RFC-0098), and the
  multi-sched-task contract (§4-§7, live: `run_sched_tasks` in 126 files, `SchedTaskSpec`).
  The banner states the reverse of today's input rule (phase-330 W4.a/W7). §8a is superseded by
  RFC-0043:222. Phases 212/228/236 are archived; status is Draft.
- **Options:** (a) amend in place: rewrite the banner, point §2 at 0091, §3 at 0098, §8a at
  0043, keep 0032 as the sched-task contract, flip to Stable; (b) move §4-§7 into RFC-0015 or
  RFC-0047 and supersede 0032; (c) Draft with notes only.
- **Recommendation:** (a). The sched-task contract has no other home and is landed. Add
  `amends: [0032]` on 0043 (handoff).

**B7. RFC-0040: which decisions survive RFC-0095, RFC-0098 and RFC-0065?**

- **Facts:** D1 is live (scaffold.rs:659-669, 1038 emit `version = "*"` only as a patch LHS;
  RFC-0064 relies on it). D2's patch rows now go to `<leaf>/build/<image>/nros-cargo.toml`.
  D3's path deps into the user's checkout meet RFC-0095 (the user installs `nros`) and
  RFC-0103 D6 (no `NROS_REPO_DIR` export); `--deps git` was never implemented. D5's "no
  build/run" is false. RFC-0120:204 reuses D-Q4.
- **Options:** (a) amend: D1 stands; D2 points at RFC-0098; D3 notes the store-provisioned
  case is RFC-0095's and the checkout path is the contributor road; delete D5's second
  clause; (b) supersede by RFC-0095 and move D1 into RFC-0097 or ARCHITECTURE; (c) keep.
- **Recommendation:** (a) now, staying Draft. Revisit (b) when RFC-0095's store-resolved
  nano-ros is built (0095 says it "does not implement that").

### Status flips to Stable

**B8. Eight Drafts whose tracking phases are all archived complete.**

- **Facts:**
  - 0067: phases 333/335/465 ("LANDED -- all five waves"); two policy-level open questions.
  - 0070: phases 334/340/344/350/383; three open questions, one being "per-profile root".
  - 0087: phase-420 "W1-W9 landed; the phase is complete"; its one open question is settled
    by the code (B14).
  - 0090: phase-429 "every work item is landed or resolved"; OQ2 settled by the code.
  - 0095: phase-440 (W1-W8 landed or closed); its end state is explicitly unbuilt.
  - 0097: phases 443 and 541; D2/D3 are "not scheduled", which is itself a decision.
  - 0099: phases 447 (all 15 items) and 517.
  - 0101: phase-471 all landed; OQ2 answered by issue 1560, OQ1/OQ3 still open.
- **Options:** flip all eight; or flip all but 0070 if its per-profile-root question is still
  wanted.
- **Recommendation:** flip all eight, recording 0097 D2/D3 as declined and 0070's question as
  open. Not flipped: 0040 (B7), 0119 and 0120 (phases 533 and 530 open). 0032 and 0065 flip
  through B6 and B2; 0048 is superseded through B1.

### Contradictions between RFCs

**B9. RFC-0119 D5 publishes `nros-extension`; RFC-0040 D1 says no `nros*` crate is published.**

- **Facts:** 0119:367 "It is published to crates.io, so an out-of-tree Rust runner uses the
  same types"; 0040:56-57 "No `nros*` crate is ever published";
  `packages/runners/nros-extension/Cargo.toml:27-30` "Prepared for crates.io (`cargo publish
  --dry-run` passes) but NOT published" (phase-527 out of scope).
- **Options:** (a) carve out in 0040 D1: the extension SDK is the one published crate (a
  protocol library, not the runtime), with 0119 `amends: [0040]`; (b) 0119 drops
  publication and out-of-tree runners depend by git; (c) rename the crate out of `nros*`.
- **Recommendation:** (a). A runner that must git-depend on the nano-ros repo defeats D5's goal.

**B10. RFC-0121 retires RFC-0119 D4's root-`system.toml` `[overrides]` fallback, unrecorded.**

- **Facts:** 0121:342 retires the fallback in favour of `nros.toml [workspace.overrides]`;
  0119 D4 (lines 255-266) and its changelog (line 560) still describe it; 0121's `amends:`
  omits 0119; phase-531 is open.
- **Options:** (a) add 0119 to 0121's `amends:` and a banner on 0119 D4 now; (b) wait for
  phase-531 to delete the code.
- **Recommendation:** (a). The decision is made; only the code lags.

**B11. RFC-0104 D9 says it replaces `check-leaf-lockfiles`, which still runs.**

- **Facts:** 0104:258 "It replaces the shim's leaf role, `check-leaf-lockfiles`, ...";
  `just/check/cargo.just:463` still defines `leaf-lockfiles` (26 drifted leaves baselined);
  CLAUDE.md states its invariant. D9's changelog (line 362) does not mention the gate.
- **Options:** (a) amend D9: the tracked-lock gate stays for in-repo leaves; (b) retire the gate.
- **Recommendation:** (a). Measure what the drift check covers before retiring anything.

**B12. "Store first" (0095 D4) vs "store last" (0099 D3).**

- **Facts:** 0095 D4 resolves provisioned workspaces as `$NROS_<X>_WORKSPACE` → store →
  checkout-relative. 0099 D3 resolves the nano-ros SDK root with the store rung last, behind
  the checkout rungs, because of 0095 D5's ownership guard. Different subjects, both live.
- **Recommendation:** one sentence in 0099 D3 naming 0095 D4 and the difference in subject.

**B13. RFC-0085 D4: the application named by a `DEPLOY` declaration inside the package.**

- **Facts:** D4 picks an image's application as the package whose manifest says
  `deploy = "zephyr"` / `DEPLOY zephyr`. RFC-0121 D1 and RFC-0130 D1 say a package names no
  board. D4's own 2026-10-24 measurement says no example image uses it; four test fixtures do.
  `check-node-package-selection-free` (RFC-0121) holds the node-package side.
- **Options:** (a) amend D4 to "generated per image; a hand-written app is named by
  `[image.<id>] entry`" and retire the arm; (b) keep the arm as an escape for hand-written
  west apps only.
- **Recommendation:** (b), worded as an escape. A hand-written west app is an application,
  not a node package, so the rulings are not broken.

### Open questions the code answered

**B14. Close answered open questions with one dated line each.**

- 0087: the `<nano_ros deploy board rmw/>` triple was retired in phase-445 W3b; a leftover
  element is a FATAL_ERROR (NanoRosPackageXml.cmake:182-186).
- 0090 OQ2: configure-time emitters read `codegen_version.rs`, not `nros --codegen-version`
  (NanoRosCodegenCore.cmake:250-294).
- 0068 OQ1 (minijinja) and OQ3 (bundled + `--template-dir`/`NROS_TEMPLATE_DIR`, Amendment 4).
  Its Serialization section's `nros dev codegen export` verb was never built (codegen.rs has
  rust, system, resolve-deps, cyclonedds-descriptors, entry, entry-node, entry-pack,
  source-language).
- 0101 OQ2: answered by issue 1560's resolution.
- **Recommendation:** add the lines; strike the export verb as "not built, no consumer".

**B15. Amend stale decision text where no choice is involved.**

- 0014: one banner listing 0062/0090/0095/0099 as amendments, and RFC-0075 for the router
  (no `zenohd` dist, `nros-sdk-index.toml:450-452`); drop the `[tool.zenohd]` example.
- 0023: record Stage C as met by `nros ws sync` (archived phase-210), Stage D as closed by
  `nros-build`'s `generate_run_plan` and RFC-0067, Stage E as `nros dev ws env`; name RFC-0090's
  stamping.
- 0054: record the layout-test reversal (32-bit targets) and the third surface (`board.h`).
- 0070 R1: drop `zenohd` from the `tools/` line and add RFC-0075 to "relates to". R5's dated
  table stays.
- **Recommendation:** make these in W3 without further ruling; they were left out here only
  because they rewrite decision text.

### Cross-reference with the rmw-wire brief

**B16. Does RFC-0042 D1 depend on RFC-0035's `abi_version`?**

- **Facts:** the rmw-wire brief's B7 recommends dropping RFC-0035's mandated `abi_version`
  (it never landed). RFC-0042 D1 is about the platform interface: one canonical
  `<nros/platform.h>` owned by `nros-platform-api`. `git grep -n abi_version
  docs/design/0042-*.md` returns nothing, and 0042's compatibility section (lines 315-317)
  says "the runtime vtable ABI (RFC-0035) ... untouched".
- **Recommendation:** the two are independent. Whatever is ruled on rmw-wire B7, no 0042
  change follows.

## Handoffs to other areas

| File | Change | Evidence |
|---|---|---|
| RFC-0130 (PR #3634) | Add `supersedes: [0048]` (B1) and `0098`, `0101` to `amends:` (B3); 0065's plain-text "Amended by" mention becomes a link | 0130 already `amends: [0065, 0024]` |
| RFC-0121 (workspace-system-model) | Add `0119` to `amends:` (B10) | 0121:342; :117 "RFC-0119 D4 as amended" |
| RFC-0043 (runtime-api) | Add `amends: [0032]` | 0043:222 "this RFC supersedes that sub-decision" (0032 §8a) |
| RFC-0040 (this area, W3) | Banner on D1 for the `nros-extension` carve-out, if B9 (a) is ruled; 0119 gains `amends: [0040]` | B9 |
| RFC-0119 (this area, W3) | Banner on D4 pointing at RFC-0121 | B10 |
| RFC-0088 (rmw-wire) | Replace the stale symbol `image::DEPRECATED_DEPLOY_FIELDS` (line 283) | `git grep DEPRECATED_DEPLOY_FIELDS` finds only that doc line |
| RFC-0062 and RFC-0087 (W3) | `supersedes: [0013]`, only if their prose is amended to say so | RFC-0013 says `superseded-by: [0087, 0062]`; neither newer RFC says it in prose |
| RFC-0090 (optional) | "Amended by RFC-0097" banner | 0097 `amends: [0090]`; 0090 never names 0097 |
| RFC-0071 (rmw-wire), RFC-0087 (optional) | "Amended by RFC-0094" banner (0071 D5 index load-bearing; 0087 D2 `build_type` gains a reader) | 0094 `amends: [0065, 0071, 0087]`; neither cites 0094 |
| RFC-0030 (rmw-wire) | Archive per B5; its area already reads `rmw-wire` | B5 |
