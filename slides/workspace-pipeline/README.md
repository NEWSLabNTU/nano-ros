# Inside the nano-ros workspace — talk

30-min talk: how the workspace turns C/C++/Rust node packages + a launch file
into a running embedded binary. Two acts — what you write, then what the
machine does (metadata → launch scan → codegen → link → schedule).

## Present

```bash
cd slides/workspace-pipeline
npm install        # first time only
npm run dev        # live at http://localhost:3030
```

No-install path: `npx slidev slides.md --open`.

Keys: `o` overview · `e` edit live · `f` fullscreen · arrows to navigate.

## Export

```bash
npm run export     # → slides-export.pdf
npm run build      # → static site in dist/
```

## Source of truth

All code excerpts are real, pulled from the tree (`examples/workspaces/*`,
`packages/cli/nros-cli-core/src/codegen/entry/*`, `cmake/*`). The generated
`main()` slide is the `emit_cpp` output shape (emitted to the build dir at
codegen, not committed). Framing refs: RFC-0001 / 0023 / 0032 / 0043 / 0016 / 0017.
