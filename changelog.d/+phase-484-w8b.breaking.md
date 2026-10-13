The 32 old build-knob env spellings (`ZPICO_MAX_QUERYABLES`, `ZPICO_FRAG_MAX_SIZE`, the `ZPICO_SMOLTCP_*` family, `ZPICO_READ_TASK_PRIORITY`, ...) are retired. Release 0.5.0-nros2 still read them with a deprecation warning. Now a build that finds one set stops with an error naming the replacement (`ZPICO_MAX_QUERYABLES is a retired spelling of NROS_ZENOH_MAX_QUERYABLES ... -- set NROS_ZENOH_MAX_QUERYABLES instead`). Where it can be set:

- exported in the environment of a cargo build, a cmake configure or a west build;
- as an `[image.<id>] env` row in `system.toml`;
- in a cargo `[env]` table.

This is true even when the canonical spelling is set to the same value. Rename each one to its `NROS_*` name. The book's deprecated-names page lists all 32. The `ZPICO_*` words remain the zenoh-pico shim's own C macro names, which are generated from the knob and are not user inputs.

A board descriptor's per-board `[board.knobs.<group>]` table is retired the same way. A descriptor that still states it no longer loads, and the error names the top-level `[knobs.<group>]` table to move it to.
