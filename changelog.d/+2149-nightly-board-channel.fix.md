`nros build` runs cargo on the pinned nightly for a board that builds with one
(`toolchain = "nightly"`, the NuttX boards), the channel its preflight already
checked. It ran the default toolchain, which ignores the board's
`[unstable] build-std`, so a NuttX Rust image failed with `can't find crate
for core`. A NuttX Rust workspace image still needs a configured kernel that
nothing provides yet (issue 2149).
