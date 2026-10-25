The generated zenoh-pico config states the knobs zenoh-pico 1.10 reads. A
ZENOH_GENERIC build takes nothing from upstream's template, so seven knobs new
in 1.10 had no value at all: the executor's task table
(`Z_RUNTIME_MAX_TASKS`, 8) and idle read sleep (`Z_RUNTIME_IDLE_READ_TASK_SLEEP`,
0 when a background executor thread runs, 1 when the caller drives it), the
link accept and connect timeouts (upstream's 1000 and 10000 ms), and three
feature flags that stay off. The 1.8 lease task's sleep chunk, the periodic
scheduler's knobs and `Z_FEATURE_SCOUTING_UDP` are no longer emitted. A read
task frame budget now rests at least 2 ms, the shortest wait zenoh-pico 1.10's
executor honours. `just check zenoh-config-template` now fails when the
template gains a knob the generator does not state.
