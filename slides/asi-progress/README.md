# nano-ros × Safety Island — progress slides

Short Slidev deck for Autoware members: ASI porting progress + the
play_launch contract / system-config split + the model-driven build pipeline.

## Run / build / export

```bash
npm install        # first time (incl. playwright-chromium for export)
npm run dev        # live at localhost:3030
npm run build      # static site → dist/
npm run export     # → slides-export.pdf (carries --dark)
```

Keys: `o` overview, arrows/space navigate (`g` goto dialog is hidden on purpose —
see `style.css`).
