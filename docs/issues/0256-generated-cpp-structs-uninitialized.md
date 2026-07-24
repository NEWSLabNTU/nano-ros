---
id: 256
title: "Generated C++ message structs are uninitialized PODs — default-init leaked stack garbage over the wire"
status: open
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
