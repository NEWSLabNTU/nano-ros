# slides/ecosystem-positioning

Slidev positioning deck. Theme seriph, dark, ~8 slides. Single file: `slides.md`.

## Audience + framing

Partner / Autoware-Foundation pitch. Angle = **layers of the stack
(complementary)**: nano-ros = RT on the safety MCU; agnocast + CIE = RT on the
Linux SoC; eSync = OTA deploy plane. **Not a head-to-head; not competitors.**

## Hard rules

- **External facts must stay accurate — do not overclaim.** Load-bearing facts:
  eSync = OTA / data pipeline, **NOT** a real-time mechanism; agnocast zero-copy
  is **intra-host** shared memory (cross-host still DDS/bridge); CIE is a **Linux
  rclcpp executor**; nano-ros↔eSync OTA endpoint is **roadmap**, nano-ros↔DDS/zenoh
  + agnocast bridge is **exists**. Keep the status labels on the integration slide.
- Cite sources (see README). Reported numbers (agnocast 16/25%, CIE ~5×) come from
  the papers — keep "reported/≈".

## Run

`npm run build` validates; `npm run export` (carries `--dark`) → PDF. Export fixes
live in `style.css` (goto-dialog hide + print un-dim) — don't regress.
See [[../workspace-pipeline/CLAUDE.md]] for the sibling internal-mechanics deck.
