`nros build` now names itself as the codegen tool on every CMake configure it
starts (the cmake image and package roads, west after `--`, and the census
build), so a build dir first configured by another `nros` (an in-tree CLI, then
a release) no longer keeps running the old binary and failing "in-tree nros CLI
is STALE". A bare `cmake` re-configure still keeps the cached tool, but refuses
at once, naming the tool and both versions, when that tool's
`--codegen-version` is outside what the nano-ros tree accepts.
