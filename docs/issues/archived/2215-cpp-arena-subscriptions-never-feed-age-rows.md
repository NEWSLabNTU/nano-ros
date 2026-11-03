---
id: 2215
title: "C/C++ arena subscriptions never feed their age row, so a C++ image reports silence at arming and its hazard observer stays latched"
status: resolved
type: bug
area: [executor, monitor]
found: 2026-11-02
related: [phase-462, issue-2173, issue-0758]
resolved_in: "fix/2215-cpp-stamp-age"
---

## The gap

The silence lease and the hazard observer read `SubMonitorCell::takes`,
which only the Rust typed paths fed (`arena::observe_age`, and only with an
epoch source). The C/C++ callback path attached no age cell, so a C++
component's contracted subscriber recorded nothing: on the safety island's
board every act read `silence-runtime` at arming and `hazard-detected` that
never recovered, and no rung route was judged during an act.

## Resolution

The stamp travels through the C++ registration:

- the C++ message pack states `static constexpr uint16_t STAMP_OFFSET` for a
  stamped type (the same predicate as the Rust pack's
  `RosMessage::STAMP_OFFSET`); `rclcpp::stamp_offset<M>` reads it, 0 when
  absent;
- every typed C++ registration passes it in
  `nros_cpp_subscription_options_t::stamp_offset` (two of the three reserved
  bytes; size and offsets unchanged), and `nros_cpp_subscription_register` /
  `_capturing` bind the entry to its topic's age row
  (`Executor::bind_raw_subscription_stamp`);
- the C entries peek the stamp at that offset on every take and record it as
  the Rust paths do: with an epoch source the age as well, without one (the
  board) the stamped take alone (`SubMonitorCell::observe_unaged`), so the
  silence rule has its evidence and `max-age-runtime` judges nothing there.
  An unstamped (or zero-stamp) take is still not evidence; the Rust typed
  paths follow the same no-epoch rule now.

Tests: `a_cpp_subscription_feeds_its_age_row_from_the_stamp` (nros-node),
`a_stamped_message_states_its_stamp_offset_in_the_cpp_header`
(rosidl-codegen). Not bound: the `_with_info`, `_message_info` and
`_validated` C++ registration variants, whose entries are other shapes.
