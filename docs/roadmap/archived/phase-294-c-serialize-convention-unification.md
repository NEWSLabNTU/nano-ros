# Phase 294 — unify the generated C serialize convention (issue #228)

Status: **Complete — 2026-07-17** (W1–W4 same-day) · Resolves issue #228 · Related: RFC-0023
(message generation), the deep-audit D-lane finding (2026-07-17).

**Goal.** The generated C `_serialize` functions use two incompatible
conventions for the same concept: messages return `0/-1` with a
`size_t* serialized_size` out-param, while services (Request/Response) AND
actions (Goal/Result/Feedback — wider than the issue filed) return the byte
count directly. The message convention is the ecosystem norm (C messages,
the C++ FFI for both msg and srv, the Rust core's size-out shape); converge
srv+action onto it.

**Break policy.** The signature GAINS a parameter, so every old call site
fails to compile — a loud break, not a silent semantic drift. No deprecated
alias: all consumers are in-tree (~16 service sites + the C action
examples), the source distribution is young (phase-288), and a changelog
note covers external users. rclc has no serialize-to-buffer analogue to
mirror, so the in-house convention just has to be SINGULAR.

## Waves

### W1 — templates — DONE
- [x] `service_c.h.jinja` + `service_c.c.jinja`: Request/Response
      `_serialize` → `(msg, buffer, buffer_size, size_t* serialized_size)
      -> 0/-1`, matching `message_c.*.jinja` verbatim (doc comment
      included). `_deserialize` already matches — untouched.
- [x] `action_c.h.jinja` + `action_c.c.jinja`: same for Goal/Result/
      Feedback (5 emitters total converted).
- [x] rosidl-codegen tests 116/116 (no signature assertions existed to
      update — noted as an E-lane gap for a future audit).

### W2 — regenerate + migrate consumers — DONE
- [x] No checked-in C service/action typesupport exists — it generates at
      build time into build dirs; regen = the fixture rebuilds in W3.
- [x] Migrated all 28 consumer files (scripted, per-file verified): the
      service call sites (native, qemu-arm-freertos,
      qemu-arm-nuttx, threadx-linux, qemu-riscv64-threadx C
      service-server/-client, service-client-callback, workspaces/c +
      mixed AddClient/AddServer) and the C action examples' goal/result
      serialize calls: `int32_t len = X_serialize(m, buf, cap)` →
      `size_t len = 0; int32_t len_rc = X_serialize(m, buf, cap, &len)`
      with guards moved to the rc (both `< 0` and value-positive guard
      variants) and `(size_t)` casts dropped.
- [x] Grep gate clean — the one remaining 3-arg call is
      `examples/native/c/custom-platform`'s own local static serializer
      (self-contained platform demo, not generated).

### W3 — fixtures + runtime proof — DONE
- [x] All five fixture families rebuilt with the new-template CLI.
- [x] Lanes: 37/37 PASS — `test_rtos_service_e2e` + `test_rtos_action_e2e`
      across freertos/nuttx/threadx-linux/threadx-riscv64 (all langs) +
      native_api service tests (3 sweep-load flakies green on retry, the
      documented class). Plus in-worktree manual proof pre-push: native C
      service pair (5+7=12) and action pair (full Fibonacci round-trip).

### W4 — closure — DONE
- [x] Issue #228 resolved + archived.
- [x] Book grep: no doc snippet shows the old service serialize shape.

## Acceptance
- One serialize convention across generated C msg/srv/action: `0/-1` +
  out-param. `rg 'int32_t .*_serialize\(.*buffer_size\);'` finds no
  count-returning variant in templates or generated trees.
- All five platforms' C service/action e2e lanes green on regenerated
  bindings.
