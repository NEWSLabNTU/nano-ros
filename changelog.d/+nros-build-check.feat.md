`nros build --check` type-checks a cargo image instead of building it. It
runs the same stages as a build and swaps only the final `cargo build` for
`cargo check`, or `cargo clippy` with `--check=clippy`, so the check sees the
image's generated settings and lock. Images that cmake or west build are
refused.
