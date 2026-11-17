# probe_workspace_dep_ws -- the metadata probe adds a node's workspace dependencies

Issue 2680. `dep_probe_pkg`'s component includes `period_lib/period.h`, a
header of the sibling library package `period_lib`, which it names in
`<depend>`. Its `CMakeLists.txt` links `period_lib` and does NOT add it itself:
the real workspace build adds it first (ws-order), and so must the source
metadata probe, which adds each component package to one generated project.

Before issue 2680 the probe added `dep_probe_pkg` alone and failed with
`period_lib/period.h: No such file or directory`. `just check workspace-probes`
configures this workspace under `NROS_PROBE_FAILURE=refuse`.
