Two nros-node timer-overrun tests no longer fail under a loaded gate: they
take a "quiet" step exactly one timer period long, and on the real clock a
preempted step dropped a period and read as a new stall. Both run on a
private test clock now (issue 1945).
