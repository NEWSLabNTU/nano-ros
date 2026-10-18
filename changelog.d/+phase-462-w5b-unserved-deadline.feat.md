A contract `deadline` on a subscription whose backend cannot judge it (XRCE, or
any registration that never hands the backend the deadline) is now judged on
target by `silence-runtime` over the deadline's window, instead of being refused
at boot or dropped. The decision reads the backend's own
`supported_qos_policies` when the subscription is created, and is stated at
boot. Age rows carry the contract's `deadline_ms` on all three roads
(`nros_cpp_age_row_t` gains the field).
