Executor levels on Linux can now be verified against real-time guarantees.
A lane that claims the new `rt-sched` capability (CAP_SYS_NICE) runs three
tests in `rt_sched_levels_e2e`: levels preempt in rank order on one CPU, a
level past its SCHED_DEADLINE budget is throttled by the kernel, and
RFC-0002 S1 (a 1 kHz loop beside 5 ms telemetry) keeps its deadlines for 60 s
(zero misses on a PREEMPT_RT kernel, the host's floor on a generic one). The
tests get the capability from a docker container granted `SYS_NICE`, so they
need no `sudo`; host-tests' new `rt-sched` job is the lane that claims it, and
everywhere else they skip by lane. The book has a new chapter, "The Real-Time
Execution Model".
