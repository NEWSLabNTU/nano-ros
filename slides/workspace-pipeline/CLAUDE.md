# slides/workspace-pipeline

Slidev talk: **"Inside the nano-ros workspace — from source to scheduled
callback."** Audience = Autoware / prospective contributors. ~45 min, ~37
slides, dark theme. Single-file deck: `slides.md`.

## Run / build / export

```bash
npm install                 # first time (incl. playwright-chromium for export)
npm run dev                 # live at localhost:3030  (--remote → /entry/ phone control)
npm run build               # static site → dist/
npm run export              # → slides-export.pdf  (already includes --dark)
```

Serve for remote viewing: `npx slidev slides.md --remote --port 3030` (binds
`0.0.0.0`; LAN/VPN only, no public tunnel). Keys: `o` overview, arrows/space
navigate. `g` (goto) is **hidden** on purpose — see below.

## Deck structure (acts)

0. Frame — what nano-ros is · 3-axis space (RMW × platform × ROS edition)
1. **Migrate** — rclcpp → nano-ros: steps · API map · before/after (real
   Autoware `HazardLightsSelector`) · CMake/`package.xml` · RTOS reality
   (freestanding C++ · malloc/heap · threading)
2. **Governance** — agents+humans · RFC→survey→phases→test→issues pipeline ·
   copyable `AGENTS.md` skeleton · doc series · formats · dev loop · versioning
3. **Authoring** — examples · node/bringup/entry roles · C/C++/Rust pkgs
4. **Machine** — metadata → launch scan → codegen → link → RT schedule · board adapters
5. Wrap — end-to-end `talker_pkg` trace · takeaways

## Hard rules for editing this deck

- **All code on slides is REAL — pulled from the tree, never invented.** Sources:
  authoring snippets from `examples/workspaces/{c,cpp,rust}/src/*`; pipeline
  internals from `packages/cli/nros-cli-core/src/codegen/entry/*` + `cmake/*`;
  before/after from `~/repos/autoware/1.7.1-ws/.../autoware_hazard_lights_selector`
  + `.../autoware_trajectory_follower_node`; doc formats from `docs/design/`,
  `docs/roadmap/`, `docs/issues/`; versions from `nros-sdk-index.toml`. Verify a
  symbol/API still exists before putting it on a slide — `nano_ros_node_register`,
  `register_node`, `NROS_NODE_REGISTER`, `create_publisher/timer/subscription`,
  `SchedContext` are the load-bearing ones.
- **Code-highlight line ranges must match the block.** `{all|1-5|7-9|...}` indexes
  lines *within the fence* (1-based, blank lines count). A range past the end →
  silent blank reveal. Re-count after any edit to a highlighted block.

## Export gotchas (already fixed in `style.css` / `package.json` — don't regress)

- **Goto dialog hidden.** `#slidev-goto-dialog { display:none }` — the `g`-key
  panel overlays the slide and a remote/touch client can miss the focus-out that
  closes it, blocking content. Navigate with arrows/click/overview instead.
- **Washed-out code in PDF.** Export captures each slide at its *final* click
  step, so the line-focus directive dims every non-focused line to 0.3. Fix:
  `html.print .slidev-code .slidev-code-dishonored { opacity:1 }` — un-dims in the
  export route only (Slidev adds `.print` to `<html>`), live deck keeps its strong
  highlight. NOTE: `@media print` does **not** work — Slidev exports in screen
  emulation; the class selector does.
- **Light vs dark mismatch.** Browser deck is dark; Slidev exports light by
  default. `npm run export` carries `--dark` so the PDF matches. For PNG frames:
  `npx slidev export slides.md --dark --format png --output frames/`.
- Flat export = one page/slide at final click state; the progressive code reveal
  isn't animated. For one-page-per-click: `npm run export -- --with-clicks`.

## Files

`slides.md` (deck) · `style.css` (the export fixes above — auto-loaded by Slidev)
· `package.json` (scripts + deps) · `README.md` (run/export for humans) · this file.

`node_modules/`, `dist/`, `*.pdf`, `frames/` are build/output — gitignore, don't commit.
