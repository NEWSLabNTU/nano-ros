C API: a service server or client the executor dispatches no longer carries
its own entity storage or callback — the executor arena owns them.
`nros_service_t` is 736 bytes instead of 2368 on x86_64, `nros_client_t` 744
instead of 2376. What a C caller changes:

- `nros_service_init_with_qos(service, node, type, name, qos)` and
  `nros_service_init_with_options(..., qos, options)` no longer take
  `callback` and `context`.
- `nros_executor_add_service(exec, service)` is gone; use
  `nros_executor_add_service_raw(exec, service, callback, context)`.
- Generated typed services: `<Srv>_service_init(&service, &node, "/name")`
  no longer takes the handler; register with
  `<Srv>_executor_add_service(&exec, &service, &handler)`. Regenerate
  (`nros sync`): the codegen version is now 12 and older trees are refused.
- `nros_client_set_response_callback` must be called after
  `nros_executor_add_client`; it returns `NROS_RET_NOT_INIT` before.
- Polled servers and clients are the new types `nros_polling_service_t` and
  `nros_polling_client_t`, with `nros_polling_service_*` /
  `nros_polling_client_*` replacing `nros_service_init_polling`,
  `nros_service_take_request_raw`, `nros_service_send_response_raw`,
  `nros_client_init_polling`, `nros_client_send_request_raw`,
  `nros_client_take_response_raw` and the two `set_wake_callback`s.
