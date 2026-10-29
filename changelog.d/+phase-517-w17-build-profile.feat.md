The release asset ships `nros-build-profile` beside `nros`, and `install.sh`
puts it on PATH, so profiling a build needs no `cargo install` from the SDK
root (phase-517 W17).
