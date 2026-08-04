# AVIAN SIM v7.0 PRO

Production-grade Ground Truth simulation for RLHF/LLM training.

## Running

### Server

```bash
cargo run --release -p avian_agent --bin sim_server
```

The interactive server auto-loads `scenarios/simulation.toml` when present (its
longer `day_length_sim_s` keeps a day/night cycle legible in the viewer). Useful
flags: `--weather`, `--urban`, `--speed <n>`, `--seed <n>`, `--config <path>`,
`--headless [--frames N]`, `--output <path>` (telemetry export), `--events-file
<path>` (pre-recorded JSONL events), `--format csv|jsonl`. Explicit CLI flags win
over the scenario file; the config file carries scenario params only (world
dims, obstacle layout, population, seed, pacing, feature flags, event schedule).

### Scenario file (`simulation.toml`)

`scenarios/simulation.toml` documents every key with an inline comment — the
default arena is `32×21`, `initial_agents`/`initial_grains` set the starting
population, `day_length_sim_s` sets the day/night cycle length, and feature
flags (`weather_enabled`, `urban_obstacles`, `immigration_enabled`,
`predator_expiry`) toggle the optional systems. A custom obstacle layout is a
`[[obstacles]]` list of `kind`/`min`/`max` boxes; pre-recorded scenario events go
under `[[event_schedule]]`. Biology constants are **not** overridable from the
file (they live in `avian_core/src/calibration.rs`).

### Headless runs and benchmarks

```bash
# Headless telemetry generation (deterministic per seed).
cargo run --release -p avian_agent --bin sim_server -- --headless \
  --frames 3600 --seed 42 --output out/run.csv --urban --weather

# Performance benchmarks (release mode):
cargo run --release -p avian_agent --bin bench -- agents 500 3600    # sim perf
cargo run --release -p avian_agent --bin bench -- snapshot 500 3600 # broadcast path
cargo run --release -p avian_agent --bin bench -- checkpoint 500 1200
cargo run --release -p avian_agent --bin bench -- export 100000     # telemetry export
```

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
the source path. Since Sprint 5 the image is rendered by the WebGL renderer as the
arena floor backdrop (a `TextureLoader` plane behind the grid at
`/assets/background-no-fireflies.jpg`), not as a CSS `background-image`; the build
output (`dist/assets/`) bundles the asset with no dependency on the `D:` path. The
image is 1664×928 and ~132 KB (no further optimization needed).

## Checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # workspace tests
cd viewer && npm run typecheck  # TS only
cd viewer && npm run build      # TS + production build
```
