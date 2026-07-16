# Phase 268 — launch node identity + per-node graph (SDD)
Task W1: complete (commit 153664953, RuntimeCtx.node_identity + macro inject + create_node override; override test passes; nros-platform recompile clean; stale-diagnostics false alarm)
Task W2: implemented (commit ce32d415e, per-node NN token lazy in zenoh shim + #104 gate). WORKS ONLY ON DIRECT/EMBEDDED SHIM PATH.
Task W3: BLOCKED (commit 960f46394, e2e tests added — correctly RED). Multi-node C++ + Rust still show only /node.
  ROOT CAUSE (verified by code trace, not theory): the RMW CFFI vtable drops per-entity node_name.
  - nros-rmw-cffi `create_publisher_trampoline` (rust_adapter.rs:437) derives node_name from the
    SESSION (`session_node_name(session)`), NOT from the incoming TopicInfo. The vtable fn-ptr sigs
    (lib.rs:458-519) carry topic_name/type/hash/domain/qos — NO node_name/namespace param. So
    TopicInfo.node_name set by nros-cpp/nros-node (correctly "talker"/"listener") never crosses to
    the backend; every entity inherits the session's single name ("node" for multi-node).
  - #98 single-node /talker worked because the session name == the one node's name. Multi-node opens
    the session generic "node" -> all entities tagged "node" -> W2 dedups to one "node" token =
    primary -> gate no-op -> only /node. EXACTLY the observed result, both languages.
  - cpp_robot_entry binary DOES contain W2 symbols (ensure_node_liveliness/per_node_liveliness) — not
    stale. (Rust native_entry fixture WAS stale, built 06-25 pre-W1/W2 — but rebuilding hits the same
    CFFI wall.)
  FIX REQUIRES extending the RMW vtable ABI (RFC-0035 frozen 34-slot, abi_version) to carry
  per-entity node_name (+namespace) across create_publisher/subscriber/service_server/service_client
  — caller (CffiRmw lib.rs) + 4 trampolines + every C/C++ backend impl + abi_version bump. The deep
  CFFI change W2 was scoped to avoid; contained-shim approach structurally insufficient for the
  hosted/CFFI path. DECISION POINT — surfaced to user 2026-06-29.
Task W2b: RESOLVED WITHOUT ABI CHANGE (in progress). Deeper trace found the fix is NOT a vtable ABI
  extension. `CffiSession::make_view()` builds a PER-CALL `NrosRmwSession` view; the create_* trampolines
  read node_name from THAT view (`session_node_name`). The caller already holds `topic.node_name` but
  built the view with the SESSION's name. Fix = thread the entity's node_name/namespace into the
  per-call view (new `entity_view` helper + 4 create_* sites in nros-rmw-cffi/src/lib.rs). No vtable
  signature change, no abi_version bump, no backend edits (Cyclone's publisher_create ignores
  node_name anyway; its graph is GUID-based). `cargo build -p nros-rmw-cffi` green. NEXT: rebuild
  cpp_robot_entry + rust native_entry fixtures (relink the fix), re-run W3 tests — expect /talker +
  /listener. User chose "design first"; design recorded here + this turn's narration.

# Phase 277 — examples UX overhaul (SDD, branch examples-ux-overhaul)
Plan: /home/aeon/.claude/plans/bubbly-skipping-kernighan.md + docs/roadmap/phase-277-examples-ux-overhaul.md
Task T0: complete (commit 1b707fff1, phase doc filed)
Task T1: complete (commits 1b707fff1..7c05b6e25, review clean)
Task T2: implemented (commit 4732ede44, review spec-pass). Fixup dispatched for Important finding
  (ros2-comparison.md register_subscription::<M,_> not a real symbol). Minor findings for final
  review triage: (a) integration-zephyr.md ASCII tree ~:72 still shows main.c placeholder next to
  component pattern; (b) SUMMARY px4 nesting inverted vs zephyr/nuttx pattern (integration-px4 child
  of px4.md; pre-existing contradictory cross-link inside integration-px4.md); (c) snippet drops
  setvbuf line vs real Talker.c (deliberate-ok, wording "verbatim" in doc prose slightly off).
Task T2: complete (commits 4732ede44 + fixup 0cb23c335, re-verify by controller: create_subscription call shape matches executor tests)
Task T3: complete (commit 85b7d1b3d, review clean). W1 docs wave DONE.
Task T4: implemented (commit 8b8266e75, review PARTIAL — bare-literal gap, fixer dispatched). Minor: report off-by-one, drive-by clippy doc fix undeclared in commit msg.
Task T5: implemented (commit 19f02bdd6, review pending). Concerns: log-feature gating dark on embedded (wire nros/log in W3/W4 manifests); threadx board crate has NO register path (T8 trace); pre-existing clippy large_stack_arrays in nros-node storage.rs test blocks -D warnings (fix before wave gate).
Task T5: complete (commit 19f02bdd6, review clean-high). Notes for later waves: session-open log only via Executor::open/open_sized (not open_multi/open_with_rmw — W4 readiness must use open path or extend); nros/std does NOT forward nros-node/log (wire in W3/W4 example manifests); board-crate CLI lookup rides generic nros-board-* fallback + new regression test.
Interlude: pre-existing clippy large_stack_arrays FIXED (commit e9e1d9718, boxed slice). Note: cargo clippy --workspace --all-targets halts on nros-rmw-zenoh-staticlib panic-handler probe (pre-existing, use just check paths instead).
Task T4: complete (commits 8b8266e75 + fixup a222740ff, re-review approved). W4 input: constants
  TALKER_LOG_PREFIX/LISTENER_LOG_PREFIX/TALKER_READY_MARKER + talker_line()/listener_line() in
  nros-tests/src/output.rs; run-c.sh mirrors as shell vars. Pre-existing flag: qemu-arm-freertos/
  nuttx rust listener lib.rs may emit NO "Received:" line (vacuous count_pattern in rtos_e2e
  test_rtos_pubsub_e2e?) — W4 parity adds the log line; verify test then.
Task T6: complete (no commits; report tmp/sdd-277/task-6-report.md). Verdicts: (i) YES typed C
  gen-interfaces in zephyr embedded build (nros_find_interfaces(LANGUAGE C SKIP_INSTALL) after
  project(); pair _serialize with nros_cpp_publish_raw); (ii) YES threadx unconditional
  alloc+critical-section+app_main safe; (iii) SAFE cyclone unbounded string (all ddsrt); watch
  aemv8r 128KB heap sizing. Flags: threadx example CMakeLists has pre-existing
  -Wl,--allow-multiple-definition (policy conflict — W7/final review); cyclonedds-known-limitations
  doc stale re NULL-loan takes; 5.7GB zephyr-workspace provisioning kept for W3/W4.
Task T7: complete (commits 5c5f1352a,19470d51e,3f716ff45; review PASS/PASS-high). Minors for final
  review triage: (a) shared example target/ means feature-build order flips the debug binary —
  smoke "ConnectionFailed" was that, not env (document); (b) `# Phase 214.S.4.b` comment kept on
  rmw-cyclonedds lines in 4 manifests (jargon-strip rule); (c) stale /target-safety/ .gitignore
  entries in talker/listener. New bins: safety-chatter-{talker,listener}, param-chatter-talker,
  header-chatter-talker (core-only lane now builds them — more coverage). just ci deferred to wave
  integrator (T7+T8 gate).
Task T8: implemented (commit ea825a341, review pending). Whole-tree cfg acceptance grep EMPTY (W3.d met). px4/ws-safety feature names kept as inert no-ops (just/px4.just + sibling entry pkgs reference them).
Task T8: complete (commit ea825a341, review PASS/HIGH). W3 wave DONE — zero cfg in tracked examples/**/*.rs.
W3 gate: just check PASSED (exit 0) at ea825a341.
Task T9: implemented (commits 9646045b4..f78467600, 9 commits). W4 chatter parity DONE across all
  platforms: harness single-point flip + INT32_* retained-wording constants + new bins/int32-sink
  (example listener's NROS_SUB_TOPIC hack extracted; ~16 tests retargeted). REAL BUGS FIXED:
  (a) codegen C++ FFI fixed-string serialize read uninit garbage past NUL -> silently published ""
  (message_cpp_ffi.rs.jinja + ffi_lib_rs.in fixed_str helper); (b) threadx-linux cpp fixture
  resolver names stale since phase-246; (c) silent RTOS rust chatter now logs (T4 gap);
  (d) ros2/bridge interop tests still passed Int32 type strings (rate-via-echo caught it).
  NuttX C pair = documented hand-CDR-String fallback (kernel link can't take generated typed C).
  Verified live: native e2e all langs + cross-lang, ros2 humble echo interop. just check GREEN.
  ALL RTOS runtime e2e lanes env-limited on this box — each BASELINED (identical failure without
  W4): zephyr rust boot-alloc panic + TAP-less C lane; freertos/threadx-linux readiness; riscv64
  NULL c_app_main after ANY rebuild (stale-binary passes only); riscv32-nuttx nros-c AtomicU64
  build break. Report: tmp/sdd-277/task-9-report.md (concerns list for integrator).
Task T9: implemented (9 commits 9646045b4..f78467600, review pending). Incl codegen FIX 8e2076d81
  (C++ FFI fixed-string serialize read past NUL → published empty string). Concerns to convert to
  issues after review: (1) threadx-riscv64 firmware NULL c_app_main fault after any rebuild — lane
  green only on stale binaries; (2) rust RTOS pubsub fixture resolvers point at binaries no lane
  builds (pre-existing); (3) NuttX C examples can't link generated typed C interfaces → documented
  hand-CDR String fallback; (4) several ros2-interop tests soft-pass on 0 received (pre-existing
  "tests must fail" violation); (5) qemu-riscv-nuttx nros-c AtomicU64-on-riscv32 build break
  (pre-existing, baselined).
Task T9: complete (commits 9646045b4..f78467600 + fixup 3eddd79a1; review: spec pass after fixup,
  quality high). W4 DONE. Critical cyclone Int32.msg→String.msg fixed (zephyr rust pair; verified
  by pattern vs C twin — local zephyr-cyclone lane env-broken, REMOTE CI is the runtime gate).
  Codegen NUL-truncation regression test added (rosidl-codegen cpp_heap_compile_check.rs). Issues
  #127-#130 filed+committed (ef73e3278) for the pre-existing findings. Minor left: NuttX listener
  ignores CDR endianness flag (platform-consistent); zephyr build-one cyclone lane has pre-existing
  $<JOIN> generator-expression break on this box.
Task T10: implemented (9 commits b80ef5aba..6c518db20, review pending). Filed #131 (native zenoh svc/action query path broken at origin/main — REMOTE CI gates zenoh svc lane). Concerns: C-over-XRCE svc roundtrip fails at baseline (triage); zephyr/rust/service-client-async source-less leftover dir (W6/W7 cleanup); embedded declarative action clients send-only (no feedback/result seam); da0503682 test relaxation needs review scrutiny.
Task T10: review PASS-with-followups (da0503682 cleared as legitimate — #131 stays red on positive paths). Fixer dispatched: 7 generic embedded cpp node names. Minor for T12: a29ff1556 swept examples/workspaces/ws-bridge-rust/Cargo.lock in (lock policy wave sweeps it); test_service_client_timeout pre-existing '|| !is_running()' soft-pass shape.
Task T10: complete (commits b80ef5aba..6c518db20 + fixup d39476007; review PASS). W5 DONE.
W5 gate: just check PASSED (exit 0) at d39476007.
Task T11: implemented (6 commits 691bf81b1..84544b625; agent died post-report pre-final-message — report complete, just-check log PASS; review pending). Copy-out smokes PASSED live (rust /home/aeon copy + nros sync + run; C copy + -DNANO_ROS_ROOT + run). Notes: ws-* copy-out out of scope (follow-up); nuttx _entry build-std libc + stm32 talker-embassy link failures pre-existing (issue candidates).
Task T11: complete (6 commits 691bf81b1..84544b625, review PASS/PASS — copy-out smoke independently reproduced). W6 DONE. Minor: nros sync writes relative patch paths for out-of-repo copies (CLI UX nit, follow-up); collapsible_if warning in a W4 example callback (drive-by candidate).
Task T12: implemented (14 commits 31e67f6c0..61b304b9e, review pending). Phase doc CLOSED (all boxes ticked). Concerns: baremetal moved fixtures' runtime tests hit pre-existing guest alloc panic (remote CI gates); fixtures-build.sh --id silently no-ops on id-less rows (fail-loud candidate); stale stm32f4 *_pkg root-excludes noted.
Task T12: complete (14 commits 31e67f6c0..61b304b9e, review PASS/PASS-high, minors only: bins/ lock-tracking convention split; stale entry-poc path comment in o5_nav2 demo_entry; -portable README 'byte-identical' overstatement; native README case-table shorthand). W7 DONE. ALL TASKS DONE — final whole-branch review next.
Final whole-branch review: READY-WITH-FIXES (F1 docs cross-wave sweep ~15 files + 4 trivia + follow-up issues #133/#134). Fix agent dispatched. Remote-CI-gated: zephyr-cyclone runtime, threadx-riscv64 runtime (#127), native zenoh svc lane (#131), riscv32-nuttx build (#130).
FINAL: fix wave landed (a25ce3bd3, 44403b76c, 0dc70be00). End-state verified by controller:
  0 cfg in tracked examples/**/*.rs; 0 tracked examples/**/Cargo.lock; check-example-matrix.sh pass;
  README Int32 gone; mdbook clean. Branch examples-ux-overhaul = 57 commits over main, phase-277
  COMPLETE. Not merged/pushed (user decision). Remote-CI-gated: zephyr-cyclone runtime, threadx
  riscv64 runtime (#127), native zenoh svc lane (#131), riscv32-nuttx build (#130).
MERGED+PUSHED 2026-07-03: rebased 57 commits onto origin/main 5f1b83124 (5 listener-file conflicts:
  upstream callback_group ABI param × our String type-name — hand-merged; issues renumbered
  #127-#134 → #131-#138 because upstream renumbered its own dups + filed #130). Post-rebase just
  check exit 0. main ff-merged to 17e042721, pushed. Checkpoint tag phase-277-pre-rebase (local
  only) at 0dc70be00.

# Issue #135 fix (2026-07-04, on main post-phase-277)
ROOT CAUSE: build_c_shim compiled zpico.c WITHOUT the generated zenoh config (ZENOH_GENERIC) while
  the zenoh-pico library used it; 8e6a5cf2a (0096 loopback) made Z_FEATURE_LOCAL_QUERYABLE diverge
  → z_get_options_t layout mismatch → library read shim target=ALL(1) as SESSION_LOCAL(1) → queries
  never sent to router. Bisect: 8e6a5cf2a^ good / 8e6a5cf2a bad; 6601c7e52 exonerated.
FIX: commit 0f02efc7e (runner.rs shim+probe generated-config injection; stale c/platform config
  copy deleted; issue archived; CLAUDE.md pitfall). Verified 11/11 svc/action + 7/7 pubsub/cyclone
  + just check green. GOTCHA burned 2 cycles: tests resolve nros-fast-release profile fixtures,
  not debug — rebuild fixtures via just build-workspace-fixtures / --profile nros-fast-release.
PUSHED: #135 fix rebased onto 276-W2 main → 3228714f2; plus 9f000e35d fmt-fix for zephyr_entry_robot1
  (pre-existing 276-W6 drift that had pr-checks red for 3 pushes). pr-checks GREEN at 9f000e35d.
  Full test-all verdict = tonight's nightly.
#140 CHECK (2026-07-04): resolved upstream same morning — NOT a delivery bug: macro-baked entries'
  ComponentCells (register_node_borrowed → enrolled slots) were invisible to hosted-spin
  observed_callback_counts → callbacks=0 always; fix = Executor::enrolled_component_states() fold.
  Locally verified after fixture refresh: multihost_runtime_e2e PASS 1.36s + multihost_zephyr_entry_e2e
  PASS 3.95s (zephyr robot1 → zenohd → native robot2, cross-host). Gotcha: west leaf build re-staled
  the native robot2 .inputsig (shared codegen inputs) — needed a second `just native
  build-workspace-fixtures` pass. Phase-276 archived upstream; new issues #144 (tiers declare race),
  #145 (zephyr tx ceiling).

# 13x platform lanes (2026-07-04)
#134 FIXED (lane green "NuttX riscv C examples built!"). Chain of 7 stacked defects peeled:
  (1) AtomicU64→AtomicU32 in nros-c action/common.rs; (2) riscv32imac-unknown-nuttx-elf spec
  defaults panic=unwind → local target JSON w/ panic-strategy=abort; (3) same JSON sets
  eh-frame-header=false (xpack riscv ld lacks --eh-frame-hdr); + -Zjson-target-spec in cargo config;
  (4) riscv defconfig lacked ARM's PTHREAD/TLS_NELEM + urandom/crypto block (pthread_key_* undefined);
  (5) nuttx_ffi_build.rs hardcoded 14-lib link list → glob staging/*.a (riscv lacks xx/crypto/board,
  has audio); (6) riscv board cmake missed the Phase-238 COMPILE_DEFINITIONS→APP_COMPILE_DEFS ferry
  (NROS_PKG_NAME unexpanded → __nros_c_component_NROS_PKG_NAME_* undefined); (7) riscv ffi bin name
  pinned to nros-nuttx-ffi (cmake copy path expects ARM's name).
W6 REGRESSION found+fixed en route: the phase-277 W6 guard script SPLICED the NANO_ROS_ROOT guard
  INTO the multi-line set(NROS_RMW_CYCLONEDDS_MSG_TO_IDL…) in the 6 threadx-riscv64 rust CMakeLists →
  NANO_ROS_ROOT empty → add_subdirectory("") self-recursion (nano_ros/nano_ros/… dirs, cmake
  "Maximum recursion depth 1000"). Re-spliced above the set(); paren-balance sweep = 0 corrupted.
  threadx build-fixtures now green.
#131 partial (commits 4aece54f3+81e21ba04): 3 of 5 defects fixed (W6 splice ff6720447; ports+slirp
  net 4aece54f3) → CPP pubsub e2e GREEN (first threadx-rv64 zenoh runtime pass). Remaining: C images
  jalr->0 AFTER successful connect (active-session path; #138 masked-symbol suspect); rust images
  emit NO wire traffic (pcap empty; #132 never-ran combo). Issue updated.
#130 fix implemented (entry_net_init in entry_212n.rs: slirp defaults 10.0.2.30/24 gw 10.0.2.2,
  deploy ip/netmask/gateway override; urandom reseed via node::init_hardware). VERIFICATION BLOCKED
  then UNBLOCKED: just nuttx build-fixtures broke on "no matching package nros" — W6 fallout: nuttx
  *_entry configs (upstream a459d5403, pre-W6) lacked patch rows for the ROLE examples' registry
  deps + 5/6 entries still had path-style nros after my rebase merge. Fixed: entries flipped to
  version="*" + nros sync rerun (patch tables now complete).
13x SESSION WRAP (2026-07-04): just check gate result recorded below. Commits (local main, unpushed):
  89574b4ee (#134 seven-defect chain), ff6720447 (W6 splice), 4aece54f3 (#131 ports+net),
  81e21ba04 (#131 issue), d395e3922 (nuttx ARM lane un-break), 703e840dd (#130 fix + C3 nuttx
  fallback), e4fa44de3 (issue updates). Residuals filed in issues: #131 (C jalr->0 post-connect;
  rust TX-dead), #132 (nuttx-rust resolvers stale since phase-212), #130 (runtime e2e gated:
  c_nuttx_entry_e2e now RUNS 60s-timeout; was instant skip).
#132 PUBSUB GREEN (2026-07-04, commits 02fad9dd7 + 5e7940b77, pushed 5e7940b77): nuttx-rust
  test_rtos_pubsub_e2e PASS 20.7s — FIRST green nuttx-rust runtime e2e ever. Three-layer fix:
  (1) resolver retarget role-lib → *_entry images; (2) 'nros entry ready' readiness marker in
  run_entry (subscriber prints nothing until receipt); (3) stdout log::Log sink in run_entry —
  chatter examples log via log::info! which was DARK on nuttx (no sink; C printf visible, Rust
  invisible) so delivery was unobservable despite working transport (proven by pcap ARP + manual
  two-QEMU both-connect). #130 eth0 fix confirmed en route (pcap: guest ARPs 10.0.2.2, gets reply).
  Distinction for #131: threadx-rust pcap was EMPTY (no ARP) = genuine network-down, NOT dark-log
  — separate transport defect, stays documented. Checking nuttx-rust service e2e next.
#132 SERVICE GREEN (commit f9cea472a): nuttx-rust service e2e PASS 6.9s. Fix: service/action _entry
  fixture rows baked pubsub port 7452 → wrong router (svc=7462, action=7472 per 89.13 table) →
  ConnectionFailed; baked per-variant + keyed svc/action readiness on 'nros entry ready'. Action
  e2e running.
#132 FINAL (2026-07-04, pushed 875fccad0): nuttx-rust rtos_e2e — pubsub PASS + service PASS (both
  first-ever green). Action RUNS + pins #137: server fully proven (goal received/feedback/succeeded)
  but client send-only (stops at 'Sending goal', accepted=false) — capability gap, filed. just check
  gate exit 0. c_nuttx_entry_e2e (C workspace entry, 17861) still 60s-timeout — pre-existing,
  orthogonal to rust resolvers, out of #13x scope. All #13x commits pushed.

#131 C-defect ROOT FOUND (2026-07-04): gdb hbreak *0 → null call from
  <Executor as Drop>::drop+94 (slot.drop trampoline == 0). Crash is BEFORE any publish
  (executor drops prematurely on the threadx-C carrier). Added null-guard to both drop loops in
  spin.rs (Executor::drop must never jalr->0 — a real robustness bug). Testing whether guard
  unblocks C publish or just stops the crash (premature-drop root may remain).
#137 DONE + pushed (58f05c48c): 3 embedded action-clients switched to
  create_action_client_with_callbacks_for_name (seam existed since 212.M-F.23; zephyr/threadx
  already used it). nuttx action e2e PASS 8.2s. Issue archived.
#131 C DEEP-DIVE DONE (a67e11eb4, pushed): jalr->0 = null meta.drop_fn in Executor::drop+94 (2nd
  loop), valid arena data_ptr, fires before publish (early teardown; C++ green happy-path). Ruled
  out: uninit (carve None-inits), stack overflow (512KB), null-guard (non-null fn types → tautology
  optimized away; volatile masks). Root: entry written with null drop_fn — deep corruption, needs
  dedicated session w/ entry-write instrumentation. Guard reverted. Rust defect (empty pcap) separate.
SESSION SUMMARY: #134 FIXED, #130 FIXED, #132 FIXED (pubsub+svc+action nuttx-rust green), #137 FIXED
  (archived). #131 3/5 defects fixed (C++ green) + remaining 2 precisely root-caused/documented.
