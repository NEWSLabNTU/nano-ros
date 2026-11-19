C++: `node.hpp` now states what a timer or guard-condition registration keeps.
Every `create_*timer*` and `create_guard_condition` form that takes a `C* self`
or `void* context` stores that pointer and passes it back on every dispatch, so
the object must outlive the `Timer` / `GuardCondition` and must not move while
registered.
