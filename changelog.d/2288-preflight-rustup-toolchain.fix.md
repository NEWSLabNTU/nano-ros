`nros build` no longer refuses a Rust image on a prerequisite the build's
own toolchain has. The preflight checked targets and components with the
toolchain rustup picks for the caller's current directory. Run from outside
the checkout, that is the default toolchain, not the one pinned in the
workspace. It now checks from the workspace being built (issue 2288).
