A Rust image now watches the launch contract's `hazards` on target. Each hazard
is observed over the contract monitors the image already runs: a guard topic
has faulted while its subscriber's `max_age_ms` silence lease is reported or
its publisher's rate window was short. The executor reports `hazard-detected`,
`hazard-reaction` (the reacting path's `safe_state` publisher published after
the fault, timed against `ftti_ms` with `settle_ms` added) and
`hazard-recovered` on the violation drain, using the same names play_launch
uses on Linux. C and C++ images follow in a later change.
