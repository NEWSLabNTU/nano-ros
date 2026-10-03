# Audit findings — 2026-09-28 — I6 second-order: gate reach

Checklist category **I6, second-order** — *a gate must be able to fail on the
case it names* — applied to every tracked `scripts/check-*` gate on `origin/main`
at `5d9fc529ec`. The class view, the fixes and the work items are
[phase-472](../roadmap/archived/phase-472-gate-reach-sweep.md); this file is the per-gate
record, so the next run of the same method diffs against it.

**Standard:** a finding is CONFIRMED only if a mutation constructing the case the
gate names made the gate PASS (rc=0) when it should fail, with the tree restored
afterwards; nearly all carry a positive control showing the gate does fail when
the same defect sits where it reads. SUSPECTED items were not demonstrated and are
not counted.

**Totals:** 320 gates; **155 confirmed** (2 P1, 94 P2, 59 P3) + 1 dead gate; 36
suspected; 35 could not run in a bare worktree (they need built artifacts,
submodules or a CLI — each auditor listed them).

**Independently re-verified by the coordinator (14, all held):**
cargo-custom-command-depfile (P1), test-precondition-guards (P1 — the 5 live
tests), profile-board-mirror, config-header duplicate define,
sdk-store-not-enumerated (both live sites and the missing self-test),
gate-selftests' classification of check-no-vacuous-tests, cc-build-policy's live
site, the three weak-symbol sites, book-identifiers red on `main`, the
zombie-blind `ps`, `ros-humble` ×11, check-issue-index reading `open.md`, the 18
fixture `match` bypasses.

Live defects are filed as issues 1539–1548; the class sweep is phase-472 W1–W9.

Format: `severity  gate  —  finding`. "LIVE" marks a case present in the tree
today with no mutation needed.


## Bucket 00 — 18 confirmed / 4 suspected / 7 unevaluable / 27 clean / 1 dead
P1  check-cargo-custom-command-depfile.py   shape1 VACUOUS: #1304 respelled cargo as ${_x_cargo}; lookbehind rejects `_`; examines 0 cmds, prints no count. Deleting DEPFILE at all 3 named sites -> OK.
P2  check-cmake-image-policy.py             shape1: neither motivating site (rv-virt-threadx seam, nano-ros-nuttx.cmake) matches its filter; deleting both nros_apply_panic_policy calls -> OK.
P2  check-atomic-sync-writes.sh             shape1 stale list: 5 hardcoded fns; misses ProviderIndex::write (providers.json, the site its own comment names).
P2  check-board-facts-delivery.py           shape1: reads cmake/ + zephyr/cmake only; misses NuttX lane nros-nuttx.cmake (cargo, no board facts). POSSIBLE LIVE BUG.
P2  check-build-profile-literals.sh         shape1: flag scan covers no cmake; path scan misses packages/**/cmake.
P2  check-cargo-dir-knob-key.sh             shape1: tests the helper, never the 3 production callers; phase-439 W1 revert passes.
P2  check-cmake-verb-reachable.py           shape1: "referenced" = substring anywhere incl .md; issue 1218's own dead NanoRosLink.cmake passes.
P2  check-book-identifiers.py               shape1: "occurs" != "defined"; both identifiers its docstring names pass.
P2  check-board-cargo-config-shape.py       shape1: .search() = first blob only; nuttx-qemu 2nd (riscv) blob unchecked (7 of 8).
P2  check-board-name-reach.py               shape1: misses nros-board.toml names[] aliases; 6 live aliases its own verdict() rejects.
P2  check-cc-build-policy.sh                shape1 LIVE: reads packages/** only; examples/mps2-an385-baremetal/c/talker/build.rs:94 ungoverned cc::Build on main.
P2  check-ci-cli-from-source.py             CLASS: CI gates read .github/workflows but never .github/actions/*/action.yml (setup-nros-cli = SOLE CLI path).
P3  check-ci-no-fixture-tolerance.py        same class (composite actions).
P3  check-ci-no-verb-fallback.py            same class + backslash-continued lines.
P3  check-ci-doc-workflow-refs.py           path-spelled citations unchecked.
P3  check-c-knob-guard-order.py             *.c/*.h only; misses 5 C++ guards + STRESS_SIZE<16.
P3  check-capability-slot-counts.sh         pub(crate) fields uncounted.
P3  check-cbindgen-pin.sh                   dotted-key form unmatched.
P3  check-cmake-find-program-shadowed.py    CACHE exemption is substring; selftest only behind --self-test.
P3  check-book-links.py                     reference-style + titled links unexamined.
P3  check-build-tool-verbs-exempt.py        only `ws <sub>` and only cmake/**.
DEAD check-board-manifest-drift.sh          can only print "nothing to audit" (single-source forbids its input).
SIDE check-book-identifiers.py RED ON MAIN: c-api.md:72 quotes removed nros_executor_register_client().
SIDE check-book-links.py red in bare worktree (gitignored generated pages; gate doesn't say so).
No-selftest NOT justified: atomic-sync-writes, board-facts-delivery, book-identifiers, build-profile-literals.

## Bucket 05 — 18 confirmed (1 P1, 14 P2, 3 P3) / 6 suspected / 7 unevaluable / 20 clean
P1  check-test-precondition-guards.py       shape2 LIVE: exempts helper returning real value "because caller will skip!" — unchecked. zenoh_integration.rs router()->Option prints [SKIP] returns None; 5 tests `let Some(_) = router() else { return }` => PASS having run nothing. check-tests-can-fail misses it too.
P2  check-tier-spin-gap.py                  shape1: file-level `extern` prototype satisfies rule alone; removing gap from BOTH zephyr tier loops stays green.
P2  check-wait-evidence-discarded.py        shape1 LIVE: misses .or_else(..).unwrap_or_default() chain (~10 live); baseline never forced down (slack lets regrow).
P2  check-zenohd-router-skips.py            shape1 LIVE: reads packages/testing only; 6 ZenohRouter::start*().expect() in packages/rmw/zenoh tests.
P2  check-zenoh-platform-macros.py          shape1: misses config/{bare-metal,generic}, runner.rs use_bare_metal, zephyr cmake 3rd producer.
P2  check-weak-symbols.sh                   shape1 LIVE: packages/** + exact `((weak))` only; 3 unaudited sites ((weak,used) freertos_c_entry.c, 2 under zephyr/).
P2  check-tier-has-ci-owner.py              shape2: step `name:` line counts as owner; `just ci matrix build` (build-only) counts as owning tier 2.
P2  check-workflow-repo-env.py              CLASS composite actions + "activate.sh" in a COMMENT exempts. LIVE: setup-qemu-patched runs just unsourced.
P2  check-workflow-indexed-apt.py           CLASS composite actions. LIVE: setup-qemu-patched apt-installs 3 indexed pkgs.
P2  check-workflow-runner-isolation.py      runs-on ${{expr}} reads as hosted; PyYAML missing => exit 0. SECURITY-ADJACENT (fork PR -> self-hosted).
P2  check-third-party-is-submodules.sh      shape2: lookbehind excludes every rooted spelling; scan roots miss activate.sh, zephyr/, build.rs.
P2  check-test-domain-assignment.sh         shape1: plain `export ROS_DOMAIN_ID=117` unmatched.
P2  check-zenohd-flag-invocations.py        shape1: \bzenohd never matches rmw_zenohd (the real router name).
P2  check-tier-priority-plan-image.py       shape1: private ladder w/ break; misses store workspace; ratchet reason claims it sweeps both roots.
P2  check-zephyr-module-binding.py          shape1 (MY CAMPAIGN'S GATE): misses root justfile, workflows, Python. LIVE-ish: colcon build.py configuring west build w/o flag.
P3  check-workflow-setup-spelling.py        composite actions.
P3  check-zenohd-spawn-sites.sh             binding name must contain "zenohd".
P3  check-zephyr-workspace-resolvers.py     .txt/.yml suffix skip hides CMakeLists.txt + workflow run: blocks.
No-selftest NOT justified: test-domain-assignment, weak-symbols, zenohd-flag-invocations.

## Bucket 03 — 28 confirmed (21 P2, 5 P3, 2 split) / 5 suspected / 7 unevaluable / 13 clean
P2  check-nested-workspace-excludes.sh      VACUOUS: nested roots now generated+untracked -> examines 0; literal grep vs cargo prefix semantics (false positive when fed real input).
P2  check-profile-board-mirror.sh           PERMANENTLY VACUOUS: points at retired packages/codegen; skip=exit 0 every run; still registered.
P2  check-msg-dep-is-path.sh                LIVE: hardcoded 16-name list; px4_msgs (3 manifests, version="*") + custom_msgs (2) pass; dotted form passes.
P2  check-markdown-links.py                 LIVE: reference-style [label]: defs unread; RFC-0034 -> archived issue 0006, 8 live 404s.
P2  check-no-vacuous-tests.py               LIVE: tests/ only; 11 print-only tests in src/ unit modules; `return;` counted as an effect.
P2  check-ps-zombie-blind.sh                LIVE: matches shell syntax only; nros-tests/src/process.rs:442 ps -eo pid,pgid w/o stat (issue 0853 shape).
P2  check-no-unbounded-condvar-wait.sh      core/rmw/api only; platform/boards unscanned ("the next port" is its stated risk).
P2  check-no-direct-kernel-alloc.sh         comment filter drops #define + `: *mut` lines; missing k_calloc/k_realloc/kmm_*/heap_caps_*.
P2  check-no-alloc-image.py                 RTOS_HEAP missing k_realloc (tree used it), sys_heap_aligned_alloc, heap_caps_aligned_alloc, zalloc.
P2  check-named-lane-fails.py               rule 4 satisfied by COMMENTS; just/check/*.just not scanned; CALLS_ANY incomplete.
P2  check-nros-c-feature-agreement.py       root justfile outside GLUE_ROOTS.
P2  check-no-std-stdio.py                   writeln!(std::io::stderr()) + `use std::eprintln` pass (same zvfs recursion).
P2  check-no-tracked-file-find.sh           find ., -iname, -L, CMakeLists/package.xml kinds pass.
P2  check-nuttx-links-snapshot.sh           hardcoded 2-file consumer list.
P2  check-nuttx-shared-tree-headers.py      examples unscanned; make-syntax $(NUTTX_DIR).
P2  check-one-producer-per-tool.py          shape2: forwarding ANY tool excuses producing any other.
P2  check-orphan-generated-stamp.py         real header dirs (build/corrosion-cargo, examples/**/build/cargo) not candidates.
P2  check-posix-platform-purity.py          shape2: "guard" = any __linux__ in 12 lines above, even after #endif.
P2  check-platform-provider-features.py     global-allocator substring incl COMMENTS (the issue-0617 row).
P2  check-prelude-tiers.py                  path-qualified + glob re-exports unread.
P2  check-nextest-test-filters.py           per-predicate not per-conjunction; """ multiline filters unread (fn-level confirmed).
P2  check-no-std-entry-emission.py          empty population (roots moved) -> OK "0 producer files".
P3  check-pipefail-sigpipe-assertions.py    extension filter: .githooks/pre-push, scripts/bin/cargo, .github run: unread.
P3  check-prose-issue-refs.py               LIVE: 6 dangling ids in unscanned trees (justfile, .rs, .hpp, workflows).
P3  check-no-silent-sample-drop.py          opener must fit one line; clang-format wraps; 5 live sites.
P3  check-no-board-init.sh                  multi-line use (rustfmt's layout) + re-added pub mod pass.
P3  check-message-crate-identity.py         [workspace.dependencies] unread.
P3  check-nextest-binary-filters.py         known targets include other workspaces.
No-selftest: 9 of 16 in bucket have a confirmed hole.

## Bucket 04 — 23 confirmed (3 live) / 3 suspected / 7 unevaluable / ~23 clean
P2  check-sdk-store-not-enumerated.py       LIVE: NanoRosCrossToolchain.cmake:181 GLOB+SORT DESC takes newest; riscv64-toolchain.sh:43,70 ls|sort -Vr. Fix MOVED defect out of regex reach. Docstring claims a selftest that DOESN'T EXIST.
P2  check-rmw-slot-producers.py             LIVE: 2 slots "produced" only by test/probe readers.
P2  check-rmw-doc-slot-names.py             try_recv_raw (its motivating case) resolves via comments elsewhere.
P2  check-rmw-ret-sign.py                   headline rule has NO failing path (print only); 43 of 66 status slots unwatched; 7 names gone.
P2  check-qos-profile-ssot.py               rule 6 reads traits.rs only; issue 0793 KeepAll in nros::qos passes.
P2  check-required-contexts-reportable.py   trigger NAMES only; branches/paths filter = PR #71 deadlock passes. PyYAML missing -> exit 0.
P2  check-release-manifest.py               only literal `exit 1` = fatal; exit 2 / false pass.
P2  check-single-rust-staticlib.py          single-line target_link_libraries only.
P2  check-staleness-probe-exemptions.sh     misses require_prebuilt_row_binary_fresh + stale_error_custom producers (0445 absorbing state).
P2  check-rust-targets-covered.py           nested fvp board rust_targets unread; config file SAYS reach is narrow, never widened.
P2  check-skippable-tests-tolerant.py       LIVE: reads no `mod` justfiles; zephyr-setup.just:401 bare nextest fvp_runtime_ws (5 skip!). Count prints 0.
P2  check-skip-marker-matching.py           *.rs only; original site was Python.
P2  check-required-features-reachable.py    any `--all-features` substring incl COMMENT makes every feature reachable.
P3  check-sysdep-remedies.sh                misses just/check/*.just.
P3  check-retired-submodule-refs.sh         stale exemption on retired ros-launch-resolve path.
P3  check-rmw-agnostic.py                   cfg(any(test, feature)) treated as test-only.
P3  check-ros2-daemon-queries.py            path-keyed allowlist (not path+verb).
P3  check-ros-env-spelling.py               every triple-quoted string treated as docstring.
P3  check-set-e-bare-assignment.py          extensionless scripts/bin/cargo unread.
P3  check-self-pkg-package-xml.py           requires [[component]]; selftest drives a local copy not violations().
P3  check-sdk-guard-can-fire.py             top-level just/*.just only; braced form only.
P3  check-runtime-umbrella-link-sites.py    umbrella via variable invisible.
P3  check-ret-code-citations.py             .jinja unscanned (ships into user headers).
P3  check-retired-cmake-keywords.py         Rust scaffold templates emit cmake, unread.

## Bucket 01 — 30 confirmed (20 P2, 10 P3) / 7 suspected / 2 unevaluable / 20 clean
P2  check-config-header-producers.py        LIVE PRECONDITION: nros_config_generated_nuttx.h defines NROS_CODEGEN_VERSION TWICE (28,113); gate reads FIRST, gcc uses LAST. Half-applied bump passes, gcc sees 8. Issue 1115 again.
P2  check-config-fallback-macros.py         same first-match-vs-last-def.
P2  check-doc-recipe-refs.py                LIVE: 24 dead `just` refs outside scope incl the retired `build-zenohd` recipe (its own motivating example) in nros-c/cpp docs.
P2  check-feature-set-ssot.sh               LIVE: zephyr/ unscanned; zephyr/CMakeLists.txt hardcodes ros-humble x9; nros_generate_interfaces.cmake:133 default "humble".
P2  check-dist-floors.py                    LIVE: reads [tool.*] only; 3 [rust.rustup].dist rows have no floor.
P2  check-emitter-just-spelling.sh          LIVE: zephyr/ + drivers unscanned; bare `just setup-cli` text lives there.
P2  check-entry-rmw-vocabulary.py           LIVE MASK: doc comment counts as registration; delete both real xrce registrations -> OK.
P2  check-codegen-version-surface.py        blind to nros_rmw::register_type_descriptor (generated code names it).
P2  check-feature-gated-modules.sh          nros-node/src/lib.rs mods only; 4 features never built alone.
P2  check-executor-stack-floor.py           file-level for per-header rule; 2nd guard removable.
P2  check-entry-session-name.py             one producer; C boot_wrapper.jinja (issue 1003 on C path) unread.
P2  check-config-header-single-writer.py    -E copy_if_different writer unseen (issue 0978's stale source).
P2  check-codegen-tool-reconfigure.py       hardcoded verb list; ws sizing-descriptor registration removable.
P2  check-feature-contract.py               PRECEDENT `build*` prune: drops src/builder/ (14 files) + committed generated/; cfg_attr global_allocator missed.
P2  check-cyclone-backend-sources.py        COMMENTED-OUT source name counts (issue 0984 exactly).
P2  check-decoupling.sh                     stale platform list (posix-c doesn't exist); misses mps2-an385/stm32f4/esp32-qemu/baremetal-common.
P2  check-dds-isolation-symmetry.py         file-level token for per-process rule.
P2  check-cxx-compat-shim-coverage.py       authored served roots omit examples/workspaces/**.
P2  check-cpp-no-std-stdio.py               */src/** only; 212 of 285 C/C++ files outside incl 62 public nros-cpp headers.
P2  check-entry-locator-ssot.py             LIVE-ish: cmake/** only; nano_rosConfig.cmake:122 second locator rung.
P3  check-cxx-standard-floor.py             CACHE + target-property forms unread; live unexempted CXX_STANDARD 11.
P3  check-component-entity-bounds.py        `impl nros::Node for` (its own doc example) skipped.
P3  check-component-lang-vocabulary.py      get_property reader form.
P3  check-cpp-ffi-error-mapping.py          shape2 allow(unreachable_patterns) exemption; Err(_e) form.
P3  check-cross-toolchain-provenance.py     no discovery of a 5th selection site.
P3  check-declared-fact-carriers.py         COMMENTED-OUT rerun line counts.
P3  check-default-gates-run-somewhere.py    if: false / continue-on-error still credited.
P3  check-deferred-call-args.py             escaped \${target} in EVAL exempt, reaches callee empty.
P3  check-entity-slot-costs.py              hardcoded to spin.rs + action.rs.
P3  check-eyre-context-alias.sh             multi-line use group (rustfmt layout).

## Bucket 02 — 34 confirmed (16 P2, 18 P3) / 11 suspected / 5 unevaluable / ~14 clean
P2  check-gate-selftests.py                 META-GATE LIVE: `if args.self_test:` not recognised as flag guard; 17 flag-only selftests counted compliant. Also mis-baselines image-paths-apply-policy (runs inline).
P2  check-literal-domain-id.py              shape4: truncates at first `mod tests` SUBSTRING; executor/mod.rs:127 `mod tests;` hides lines 128-184 of shipped code.
P2  check-ffi-struct-mirrors.sh             hardcoded 2-struct list; nros_cpp_subscription_options_t (callback_group tail — drifted twice) unchecked.
P2  check-fixture-binary-names.py           3 resolvers listed; ~30 literal sites through others.
P2  check-fixture-require.py                LIVE: 18 `match build_x() { Err(e)=>panic!() }` bypasses; selftest exercises a different matcher than sites().
P2  check-fixture-stamp-honesty.py          text-anchor order, not behaviour; guard neutralised -> OK.
P2  check-fixture-variant-features.py       shape3 one-sided: features() checked, rmw() only checks table exists.
P2  check-interop-verdicts.py               missing tracked ledger reads as empty -> 25 verdicts erased, OK.
P2  check-issue-index.sh                    LIVE: duplicate-row arm reads generated open.md (0 rows); 329 rows in README.md. CAN NEVER FIRE.
P2  check-just-recipe-refs.py               `just <module> <anything>` always passes; just/check/*.just unread.
P2  check-just-recipe-paths.py              flat glob misses just/check/*.just.
P2  check-knob-resolved-once.py             if/else pair + 3rd unconditional call -> counted once.
P2  check-lane-contracts.py                 5 require_* only; ~320 build_* resolvers (emit the exact failure it exists for) uncounted.
P2  check-lane-scope-consumers.py           exempts native_* files, which the host lane RUNS; false claim live.
P3  check-fixture-artifact-dir-inputs.py    shape2: any flag within 25 lines exempts, cross-recipe.
P3  check-fixture-id-guard.sh               1 of 3 builders proven wired.
P3  check-gate-cache-keys-agree.py          one pair, one direction.
P3  check-generated-cmake-keywords.py       push_str only; += / write! unseen.
P3  check-generated-leaf-regenerable.sh     LIVE: proxy (package.xml) not rule; my_robot_node outside regenerator pathspecs.
P3  check-generated-schema-coverage.py      per-file any-match; "structs" count is files.
P3  check-git-dir-layout-assumptions.py     backticks stripped as prose in SHELL (= cmd substitution).
P3  check-goal-cdr-stripped.py              `pub unsafe extern` only; no count floor (3 expected, 2 accepted).
P3  check-grep-q-error-conflation.py        LIVE: cmake/ + .github/actions unscanned; 2 live sites.
P3  check-host-platform-vocabulary.py       depth-1 boards only (fvp at depth 3).
P3  check-host-triple-literals.py           rustc absent -> M3 silently skipped as OK.
P3  check-image-paths-apply-policy.sh       per-file not per-target.
P3  check-interface-glob-configure-depends.py single-line only; no floor.
P3  check-interlock-visibility.py           stated `needs` requirement never checked.
P3  check-interop-cell-runners.py           echo text counts as a runner.
P3  check-lane-coverage-labels.py           fixed phrase list.
P3  check-lane-skip-protocol.py             `|| { …; exit 0; }` idiom missed.
P3  check-leaf-lockfiles.sh                 silently drops 4 tracked locks under packages/cli/.
P3  check-ledger-key-spelling.py            unprefixed keys skipped.
P3  check-ledger-orphan-refs.py             LIVE: crate-relative paths treated as upstream; 2 dead citations.
