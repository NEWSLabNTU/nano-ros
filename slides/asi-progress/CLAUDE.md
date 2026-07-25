# slides/asi-progress

Slidev progress deck. Theme seriph, dark, ~10 slides. Single file: `slides.md`.

## Audience + framing

Autoware members who already know the nano-ros/ASI idea — this is a **progress
update**, keep it short. Content: project-relationship diagram (nano-ros ·
play_launch · ASI share one contract + system config; ASI uses nano-ros as its
RMW/ROS layer), port status, AVH deployment shape, the three-file split
(launch / contract / system.toml), build-pipeline diagram, RTOS mapper table.

## Hard rules

- **All facts from the tree** — phase-292 / phase-296 roadmap docs, RFC-0050 /
  RFC-0052 / RFC-0047 / RFC-0046, and real files
  (`examples/workspaces/ws-realtime-cpp-fvp/`, `ros-launch-manifest` fixtures).
  Status labels (✅/🔄/🚚) must track the roadmap docs — update, don't invent.
- MR-CANHUBK3 board status ("in customs") is time-sensitive — refresh before
  presenting.
- Snippets are trimmed but real; verify symbols before editing
  (`nano_ros_use_board`, `nano_ros_add_executable`, `group_tiers`, `run_tiers`,
  `create_callback_group`).

## Run

`npm run build` validates; `npm run export` (carries `--dark`) → PDF. Export
fixes live in `style.css` (goto-dialog hide + print un-dim) — don't regress.
Siblings: [[../workspace-pipeline/CLAUDE.md]], [[../ecosystem-positioning/CLAUDE.md]].
