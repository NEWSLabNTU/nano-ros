`nros-log`'s pick-one cargo features (`max-level-*`, `buffer-size-<N>`,
`early-records-<N>`, `rosout-records-<N>`, `dynamic-loggers-<N>`), deprecated
since nros-v0.6.0, are now refused: selecting one stops the build, naming the
`NROS_LOG_*` knob and the value to state instead (`[image.<id>.knobs.log]`, the
board's `[knobs.log]`, `CONFIG_NROS_LOG_*` on Zephyr, or the environment).
