# AVIAN SIM v7.0 PRO

Production-grade Ground Truth simulation for RLHF/LLM training.

## Running

### Server

```bash
cargo run --release -p avian_agent --bin sim_server
```

Useful flags: `--weather`, `--urban`, `--speed <n>`, `--config <path>`.

### Viewer

```bash
cd viewer
npm ci
npm run build
npm run preview
```

Then open `http://localhost:4173` (preview) or use `npm run start` for the Vite dev
server. The viewer connects to the sim server on `ws://127.0.0.1:8080`.

## Viewer background asset

The app background is the photo
`D:\Gemini\golebie\visuals\background no fireflies.jpg` (source; license: see that
directory). It is copied into the repo as
`viewer/public/assets/background-no-fireflies.jpg` so the build never references
the source path — the CSS uses only the relative `/assets/background-no-fireflies.jpg`
URL. The image is 1664×928 and ~130 KB (no further optimization needed). A
semi-transparent overlay keeps text and the WebGL canvas readable over the photo.

## Checks

```bash
cargo test --workspace          # workspace tests
cd viewer && npm run typecheck  # TS only
cd viewer && npm run build      # TS + production build
```
