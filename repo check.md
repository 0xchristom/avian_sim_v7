# AVIAN SIM v7 — Repository Connectivity Audit

Date: 2026-08-04
Scope: full workspace (`avian_core`, `avian_agent`, `avian_physics`, `avian_telemetry`, `viewer`, `bin/*`).
Purpose: index every written-but-unconnected artifact so future work can consult this file instead of re-scanning the whole repo.
Method: 4 parallel sub-agent audits + manual verification of every flagged item via grep.

## Legend

- **ISLAND** — defined, referenced nowhere in the workspace (incl. tests).
- **TEST-ONLY** — consumed only by `tests/` or self-tests; not in production paths.
- **SERVER-ONLY** — wired to a binary (`sim_server`/`bench`) but not the engine hot tick.
- **PARTIAL** — reachable, but only via an unusual/non-obvious path.
- **VIEWER-ONLY** — present in viewer code but never rendered/consumed.
- **CONNECTED** — wired end-to-end (listed only where it corrects a false positive).

## Hot-path baseline (what IS connected)

`Simulation::step` (`avian_core/src/lib.rs:829`) → `tick_fn` = `run_systems` (`avian_agent/src/systems.rs:68`), supplied by `sim_server.rs:251,325` and `bench.rs`. Everything reachable only from `run_systems` is CONNECTED:
- `update_thermals` (systems.rs:82), `weather::update` (:85, config-gated off by default), `metabolism_system` (:115), `predator::{collect_threats,plan_movement,resolve_contact,tick_lifetimes}` (:188,739,841,845), `build_default_tree` (:221) → `tree.tick` (:570), `perception::{cone_cast,normalize_angle_relative}` (:433,:491), `flocking::{default_weights,weights_for_state,steering}` (:596), `locomotion::HeadBobSystem::update` (:655), `gerontology::{mortality_hazard,spawn_agent}` (:321,:901), `search::{levy_step,crw_direction}` (via behavior_tree.rs:39), telemetry `state_to_observation`/`RLReward::compute`/`exporter.push` (:945,:955,:981, gated on `--output`).

---

## TRUE ISLANDS (dead code, zero callers anywhere)

| # | Symbol | Location | Why unused / notes |
|---|---|---|---|
| I1 | `SearchMode` enum + `next_step()` | `avian_agent/src/search.rs:4,20` | Behavior tree uses only `levy_step`/`crw_direction` (behavior_tree.rs:39). `next_step` is a superseded search API. |
| I2 | `perception::local_enhancement_score()` | `avian_agent/src/perception.rs:46` | Zero callers. Presumably an unimplemented "social facilitation" model. |
| I3 | `flocking::neighbors_in_radius()` | `avian_agent/src/flocking.rs:106` | Zero callers. `systems.rs` does its own neighbor filtering. |
| I4 | `rlhf::RLAction` + `DiscreteAction` | `avian_telemetry/src/rlhf.rs:8,13` | Never referenced, not even in tests. Action-space type designed but never emitted. |
| I5 | `exporter::flush_to_csv()` | `avian_telemetry/src/exporter.rs:295` | Zero callers (CSV flush path superseded by `flush_buf`/`write_frame`). |
| I6 | `StaticObstacleBroadphase::is_empty()` | `avian_physics/src/lib.rs:138` | Zero callers, even internally. |
| I7 | Calibration `FEATHER_CONDITION_DEFAULT` | `avian_core/src/calibration.rs:180` | Not even exported by `calibration_export_json`. No consumer. |
| I8 | Calibration `HATCHLING_MASS_G` | `avian_core/src/calibration.rs:16` | Only self-assert :520 + export. |
| I9 | Calibration `DAILY_ENERGY_REQUIREMENT_KJ` | `avian_core/src/calibration.rs:63` | Only self-assert :578 + export. |
| I10 | Calibration `BINOCULAR_OVERLAP_DEGREES` | `avian_core/src/calibration.rs:68` | Only self-asserts :586-587 + export. |
| I11 | Calibration `OBS_MEMORY_COUNT` | `avian_core/src/calibration.rs:394` | Only export. `rlhf.rs` never reads it (see metadata.rs:29 "all zero until it ships"). |
| I12 | `FSMState::Idle` variant | `avian_core/src/components.rs:56` | Never set by any code path. Spawn defaults to Spacer (gerontology.rs:98); tree never emits Idle. Appears only in `as_str()` (:84) and `ALL` (:99). |

## TEST-ONLY (have consumers, but only in tests)

| Symbol | Location | Consumer |
|---|---|---|
| `locomotion::VaultingGait` + `com_height` | `avian_agent/src/locomotion.rs:5,12` | `tests/biomechanics.rs:1,5` |
| `union_find::component_size` | `avian_agent/src/union_find.rs:53` | union_find unit tests :71-88 |
| `PhysicsWorld::dt()` | `avian_physics/src/lib.rs:461` | `avian_core/tests/config.rs:338,354` |
| `PhysicsWorld::reset_raycast_count()` | `avian_physics/src/lib.rs:316` | tests/obstacles.rs:115,133,144; urban_map.rs:268 |
| `PhysicsWorld::los_raycast_count()` (method) | `avian_physics/src/lib.rs:323` | tests/obstacles.rs:121,138,146,219; urban_map.rs:270 |
| `StaticObstacleBroadphase::len` | `avian_physics/src/lib.rs:134` | tests/obstacles.rs:204 |
| `SimRng::sample` | `avian_core/src/rng.rs:59` | rng self-tests only; `gen_range` used everywhere else |
| `TelemetryExporter::open` (CSV-only) | `avian_telemetry/src/exporter.rs:173` | bench.rs:287 (`bench export`); not sim_server |

NOTE: `los_raycast_count` (the field) IS incremented in production (`cast_ray_to_static`, lib.rs:296) — only the read-back is test-only.

## PARTIAL / SERVER-ONLY (reachable, but outside the tick or only via unusual path)

| Symbol | Location | Path | Gap |
|---|---|---|---|
| `metrics::compute_metrics` + `Metrics` | `avian_agent/src/metrics.rs:43` | sim_server.rs:439, interactive broadcast only (~100 frames) | Absent from headless + bench → `union_find` dead there |
| `union_find::UnionFind` | `avian_agent/src/union_find.rs:13` | only via `compute_metrics` (metrics.rs:118) | Dead in headless/bench runs |
| `scripted_population::ScriptedGrowth` | `avian_agent/src/scripted_population.rs:56` | driven by server loop sim_server.rs:190,256,329 | NOT in `run_systems`; embedders using `Simulation::step(run_systems)` alone silently lose it. No hook in systems.rs |
| `Simulation::save_checkpoint` / `load_checkpoint` | `avian_core/src/lib.rs:946,969` | bench.rs:228,232 + tests only | **Server exposes no CLI flag or WS command**; full replay subsystem with zero runtime wiring |
| `Event::TeleportAgent` / `KillAgent` | `avian_core/src/lib.rs:791,813` | generic `inject_event` only (`--events-file`, toml `event_schedule`, WS JSON sim_server.rs:377-380) | No first-party emitter; viewer never sends them |
| `Event::RemovePredator` / `SetWeather` | `avian_core/src/lib.rs:770,782` | same generic inject path | Engine emits RemovePredator internally (predator.rs/tick_lifetimes); SetWeather reachable only via injection |
| `systems::spawn_grain` | `avian_agent/src/systems.rs:20` | sim_server.rs:213,392; bench.rs; tests | Not on the tick (setup/utility) |
| `weather::update` | `avian_agent/src/weather.rs:18` | hot tick systems.rs:85 BUT `config.weather_enabled` off by default | Feature flag never on for default runs |
| `--frames` CLI flag | `avian_agent/src/bin/sim_server.rs:43-48` | headless only | Silently ignored in interactive mode |

## VIEWER dead fields & cosmetic gaps

| Item | Location | Status |
|---|---|---|
| `Metrics.survival` | `viewer/src/store/useSimulationStore.ts:73`, `components/Dashboard.tsx:38` | Server computes+streams (metrics.rs:101-108); Dashboard never renders → wasted bandwidth/compute |
| `Metrics.fsm` | store:74, Dashboard:39 | same |
| `Metrics.predator_count` | store:68, Dashboard:33 | Dashboard uses `ui.predators?.length` instead |
| `Metrics.agents` | store:63, Dashboard:26 | Dashboard renders `displayedAgents.length` instead |
| `SimulationSnapshot.agent_count` | store:48; ui:133 | never read by any component (App uses `ui.agents.length`) |
| `ui.weather` / `ui.weather_intensity` / `ui.obstacles` / `ui.agent_count` | store:128-133 | set in throttle rebuild but renderer reads them from `snapshot`, not `ui` |
| Dashboard `FSM_COLORS` keys `Resting`,`NightRest`,`Wandering`,`CriticalEnergy` | `Dashboard.tsx:54-64` | don't exist as FSMState variants → those states fall to `#888` |
| WebGLRenderer `FSM_COLORS` missing `Gliding` | `WebGLRenderer.ts:4-13` | gliding pigeons render fallback gray |
| No `WebGLRenderer.dispose()` | WebGLRenderer.ts | App cleanup (App.tsx:99-102) closes WS but never frees Three.js resources |

## Corrections (false positives flagged by sub-agents, manually verified as USED)

- `VISION_MAX_RANGE_M` — USED at `systems.rs:111`. NOT an island.
- `DAY_LENGTH_SIM_S` — USED at `avian_core/src/lib.rs:141` (config default). NOT an island.
- `ADULT_BMR_WATTS` — transitively live via `bmr_for_mass` (calibration.rs:123), called by digestion.rs:40. NOT an island.
- `VITALITY_T_MID_YEARS` / `VITALITY_SHAPE_P` — indirect-only via `vitality_at` (gerontology.rs:20,29; biology_validation.rs:154). Live, not orphaned.
- `predator_expiry`/`predator_fill_meals`/`immigration_enabled`/`urban_obstacles`/`scripted_population` config flags — all genuinely wired (sim_server.rs:107-120, 115-117, 188-194).

## Wire-format notes (server ↔ viewer)

- Outbound coalesced `{snapshot,event_log,metrics}` — structurally identical to viewer interfaces; only dead fields listed above.
- Inbound `command pause/resume/step/speed`, `spawn_grain,x,y`, `{event:spawn_predator}` — all handled.
- Enum string encodings line up (fsm_state/hunt_state/weather/obstacle kind).
- Soft coupling: viewer hard-codes 32×21 world dims (WebGLRenderer.ts:117-118,179,395-398); any scenario changing `world_width/height` desyncs viewport math. No world-size field on the wire.
