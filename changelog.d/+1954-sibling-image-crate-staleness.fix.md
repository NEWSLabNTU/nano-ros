The fixture freshness probe no longer reads a sibling leaf's generated image
crate as an input. Since each C/C++ leaf builds its own image crate, the shared
cargo group dir holds one folder per leaf, and the probe walked all of them, so
a sibling configured later in the same fixture pass made every FreeRTOS C++
fixture read STALE. The probe now walks only the target dirs the leaf's own
`build.ninja` names.
