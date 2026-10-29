`nros setup` at a workspace root now asks only for the OS packages that
workspace names: its build types' host tools and any `<depend>` that is a
system key, instead of every board's toolchain in the index. It also refuses a
`<depend>` it cannot resolve before fetching anything, with the same check and
message `nros build` uses (phase-517 W4).
