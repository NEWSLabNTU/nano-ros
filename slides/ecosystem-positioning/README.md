# slides/ecosystem-positioning

Slidev talk: **"Where nano-ros fits — real-time to the edge, OTA-ready."**
Partner / Autoware-Foundation positioning pitch (~8 slides). Places nano-ros
alongside **agnocast**, **CallbackIsolatedExecutor (CIE)**, and the **eSync
Alliance** OTA pipeline — complementary, different planes of one SDV stack.

## Run / build / export

```bash
npm install                 # first time (incl. playwright-chromium for export)
npm run dev                 # live at localhost:3030
npm run build               # static site → dist/
npm run export              # → slides-export.pdf (already --dark)
```

Serve for remote viewing: `npx slidev slides.md --remote --port 3031`
(binds `0.0.0.0`; LAN/VPN only).

## Sources (all claims are externally verifiable)

- eSync × Autoware OTA working group / Open AD Kit — esyncalliance.org, autoware.org
- agnocast (true zero-copy IPC) — arXiv 2506.16882, github.com/autowarefoundation/agnocast
- CallbackIsolatedExecutor — arXiv 2505.06546 (RTAS 2025), github.com/autowarefoundation/callback_isolated_executor
