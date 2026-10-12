C API: generated messages take rosidl's C names and member shape, and the
type support is rosidl's macro (RFC-0089, phase-492 W7/W8). Codegen version 14;
regenerate C trees (`nros sync`). What a C caller changes:

- Names: `std_msgs_msg_string` is `std_msgs__msg__String`, included as
  `<std_msgs/msg/string.h>` (rosidl's snake-case file name). Services are
  `<pkg>__srv__<Name>_Request` / `_Response`, actions
  `<pkg>__action__<Name>_Goal` / `_Result` / `_Feedback`. Functions take `__`:
  `<Msg>__serialize`, `__deserialize`, `__deserialize_view`, `__View`;
  constants `<Msg>__<CONST>`, bounds `<Msg>__TX_MAX_SERIALIZED_SIZE`.
- Member shape: a string is `rosidl_runtime_c__String {data, size, capacity}`,
  a sequence `rosidl_runtime_c__<elem>__Sequence` / `<Msg>__Sequence`, over
  memory the CALLER owns. `msg.data` is `msg.data.data`; set `.size`. Wire a
  message with the generated `<Msg>__init_in(&msg, &storage)` over a
  `<Msg>__Storage`, or point `data` at your own buffer (micro-ROS's idiom).
  A message with no strings or sequences needs no wiring (`= {0}`).
- `<Msg>_init` / `_fini` are gone, and rosidl's allocating `__init` / `__fini`
  / `__create` / `__destroy` are not declared (a call fails to compile naming
  `__init_in`). Deserialising writes into `capacity` and fails when a field
  does not fit; it never allocates. RFC-0033 `heap` mode no longer applies to
  C (a `heap` entry only sizes `__Storage`); Rust and C++ are unchanged.
- Type support: `<Msg>_get_type_support()` is
  `ROSIDL_GET_MSG_TYPE_SUPPORT(pkg, msg, Name)` (and `_SRV_` / `_ACTION_`),
  from `<rosidl_runtime_c/message_type_support_struct.h>`. It yields nano-ros's
  descriptor, which now carries `serialize` / `deserialize` (services and
  actions per part). `rosidl_message_type_support_t` stays undeclared.
- Publish with `rcl_publish(&pub, &msg, NULL)`; the generated `<Msg>_publish`
  is gone. Register a typed subscription with rclc's six arguments,
  `rclc_executor_add_subscription_with_context(&exec, &sub, &msg, cb, ctx,
  invocation)`; `rclc_executor_add_subscription_typed{,_sized}`,
  `<Msg>_executor_add_subscription*` and `<Msg>_subscribe*` are gone. A polling
  subscription's typed take is `rclc_polling_subscription_take(&sub, &msg,
  NULL)`.
- Generated service helpers keep their shape under the new names
  (`<pkg>__srv__<Name>__service_init`, `__executor_add_service`,
  `__service_handler_t`, …); their handler structs now carry the payloads'
  `__Storage`.
