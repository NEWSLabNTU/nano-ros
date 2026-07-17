# Phase 294 — unify the generated C serialize convention (issue #228)

Status: **In progress — 2026-07-17** · Resolves issue #228 · Related: RFC-0023
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

### W1 — templates
- [ ] `service_c.h.jinja` + `service_c.c.jinja`: Request/Response
      `_serialize` → `(msg, buffer, buffer_size, size_t* serialized_size)
      -> 0/-1`, matching `message_c.*.jinja` verbatim (doc comment
      included). `_deserialize` already matches — untouched.
- [ ] `action_c.h.jinja` + `action_c.c.jinja`: same for Goal/Result/
      Feedback (+ any synthesized wrapper serializers that forward).
- [ ] Codegen unit tests: update the template-output assertions in
      `rosidl-codegen` (grep the emitted signature), run the crate's tests.

### W2 — regenerate + migrate consumers
- [ ] Regenerate every checked-in `generated/` tree that carries service or
      action C typesupport (`just generate-bindings` + workspace syncs as
      needed).
- [ ] Migrate the ~16 service call sites (native, qemu-arm-freertos,
      qemu-arm-nuttx, threadx-linux, qemu-riscv64-threadx C
      service-server/-client, service-client-callback, workspaces/c +
      mixed AddClient/AddServer) and the C action examples' goal/result
      serialize calls: `int32_t len = X_serialize(m, buf, cap)` →
      `size_t n = 0; if (X_serialize(m, buf, cap, &n) != 0) …` with the
      length uses switched to `n`.
- [ ] Grep gate: no remaining 3-arg `_serialize(` call in `examples/` or
      `packages/` outside generated deserialize forms.

### W3 — fixtures + runtime proof
- [ ] Rebuild affected fixture families: native (c srv/action rows +
      workspace fixtures), freertos, nuttx, threadx-linux,
      threadx-riscv64.
- [ ] Rerun the service + action e2e lanes on all five platforms
      (`test_rtos_service_e2e` / `test_rtos_action_e2e` C cells,
      native_api service tests, workspace add-server/client lanes).
- [ ] `just check-c` (header syntax + cross-include TU) green.

### W4 — closure
- [ ] Resolve + archive issue #228; findings-log entry flips.
- [ ] Book/reference: if any doc snippet shows the old service serialize
      shape, update it (grep `_serialize` in book/).

## Acceptance
- One serialize convention across generated C msg/srv/action: `0/-1` +
  out-param. `rg 'int32_t .*_serialize\(.*buffer_size\);'` finds no
  count-returning variant in templates or generated trees.
- All five platforms' C service/action e2e lanes green on regenerated
  bindings.
