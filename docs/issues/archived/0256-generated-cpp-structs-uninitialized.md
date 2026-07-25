---
id: 256
title: "Generated C++ message structs are uninitialized PODs — default-init leaked stack garbage over the wire"
status: resolved
type: bug
severity: medium
area: codegen
related: [issue-0253]
---

## Finding (autoware-safety-island-example P1, 2026-07-24)

rosidl C++ zero-initializes message members; nano-ros generated structs are
plain PODs with no initializers. Ported upstream code doing

```cpp
OperateMrm::Response response;   // upstream shape — fine under rosidl
response.response.success = true;
return response;
```

serialized stack garbage in `response.code` (observed value 51392 on the
wire). The ports now value-init (`Response response{};`) but every future
port is one missed `{}` away from the same leak.

## Fix

Emit default member initializers in `message_cpp.hpp.jinja` (+ srv/action
templates): `= {}` per field (or `= 0` scalars). Zero runtime cost, restores
rosidl semantics.

## Resolution (2026-07-25)

All 12 field-emission sites in the C++ message/service/action templates
emit `= {}` default member initializers (aggregate-safe at the cxx_std_14
floor; C templates untouched — C has no member init). Template tests
updated (178/178); verified via the real cmake pipeline: regenerated
`std_msgs_msg_string.hpp` carries `nros::FixedString<256> data = {};`
and the native cpp example builds. Existing `generated/` trees pick the
fix up on their next regeneration (the codegen-tool input signature
already stales fixtures on a CLI rebuild).
