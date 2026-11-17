The book now shows `nros build --profile=` for a debug build, documents
`NROS_PROBE_FAILURE` (set `refuse` in CI), says that a C or C++ package's
`project()` must be its `package.xml` name, and says that a C++ parameter whose
launch value has another type than `declare_parameter<T>` stops the boot.
A new reference page, Build arguments, lists where a CMake or cargo argument
can be stated (`--cmake-args=`, `--cargo-args=`, `--profile=`,
`[image_defaults.build]` and `[image.<id>.build] cmake_args`), the order they
apply in, which one the source probe reads, and what is refused.
