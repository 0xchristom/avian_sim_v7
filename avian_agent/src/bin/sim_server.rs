use avian_agent::gerontology::spawn_agent;
use avian_agent::metrics::compute_metrics;
use avian_agent::scripted_population::ScriptedGrowth;
use avian_agent::systems::{run_systems, spawn_grain};
use avian_core::calibration;
use avian_core::events::Event;
use avian_telemetry::{write_metadata, Format, TelemetryExporter, TelemetryMetadata};
use std::net::TcpListener;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tungstenite::accept;
use tungstenite::WebSocket;

/// Audit 2 Task 1: outbound broadcast pacing interval (~60 Hz real time).
/// Decouples the network send rate from the sim step rate so speed 1×/10×/100×
/// can't flood the browser or fill the non-blocking socket's send buffer.
const BROADCAST_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

/// Audit 2 Task 1: classify a WebSocket send error. `WouldBlock` on a
/// non-blocking socket just means the OS send buffer is full (the client is
/// momentarily behind) — skip that client this broadcast, NOT a disconnect.
/// Any other error is a genuine I/O failure and disconnects the client, in
/// line with the existing read-path classification.
fn send_error_is_fatal(err: &tungstenite::Error) -> bool {
    !matches!(
        err,
        tungstenite::Error::Io(e) if e.kind() == std::io::ErrorKind::WouldBlock
    )
}

fn main() {
    // Parse CLI args: --headless [--frames N] [--output path] [--events-file path] [--seed N]
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    // 4.3: opt-in urban map (buildings/trees/water + line-of-sight occlusion).
    let urban = args.iter().any(|a| a == "--urban");
    // 4.4: opt-in stochastic weather scheduler (Clear/Rain/Wind/Heat).
    let weather = args.iter().any(|a| a == "--weather");
    // 5a item 2: opt-in scripted population growth (start 4 → 6/10/15/20 at
    // 2/5/10/15 sim-min). Also settable via simulation.toml.
    let scripted_pop = args.iter().any(|a| a == "--scripted-population");
    let frames_target: u64 = args
        .iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0); // 0 = unlimited
                       // Per-run seed — metadata.json is designed to track this per run.
    let seed: u64 = args
        .iter()
        .position(|a| a == "--seed")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);
    // Telemetry file output is opt-in via `--output <path>`. Without it the
    // server runs without writing any dataset, so it never grows a dataset.csv.
    let output_path = args
        .iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());
    let events_file = args
        .iter()
        .position(|a| a == "--events-file")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());
    // 3.4: export format. `--format jsonl` for the lossless debug format.
    let format = args
        .iter()
        .position(|a| a == "--format")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| Format::parse(s))
        .unwrap_or(Format::Csv);
    // 5.2: base scenario from a `simulation.toml` file. CLI flags below override
    // the file on collision (explicit command line wins over a file default).
    let config_path = args
        .iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());

    let mut config = match &config_path {
        Some(path) => match avian_core::SimulationConfig::from_file(path) {
            Ok(c) => {
                println!("Scenario config loaded from {path}");
                c
            }
            Err(e) => {
                eprintln!("Failed to read config file {path}: {e}");
                std::process::exit(1);
            }
        },
        // Audit 4 §9.7: the interactive server auto-loads `scenarios/simulation.toml`
        // when it exists (e.g. its longer day_length_sim_s for a legible day/night
        // cycle), so the viewer scenario works without a --config flag. An explicit
        // --config above wins; a missing file falls back to the compiled default.
        None => match avian_core::SimulationConfig::from_file("scenarios/simulation.toml") {
            Ok(c) => {
                println!("Scenario config auto-loaded from scenarios/simulation.toml");
                c
            }
            Err(_) => avian_core::SimulationConfig::default(),
        },
    };
    // CLI overrides (win over the file).
    config.urban_obstacles |= urban;
    config.weather_enabled |= weather;
    config.scripted_population |= scripted_pop;
    // Audit 5a item 2: while the scripted schedule is the population driver,
    // immigration must stay off — otherwise the 2.4 respawn-to-MIN_POPULATION
    // logic fights the schedule (e.g. topping the population back to 10 the
    // moment it dips, and adding births at frame 0). The audit explicitly says
    // the schedule has no interaction with the death/immigration logic.
    if config.scripted_population {
        config.immigration_enabled = false;
    }
    if args.iter().any(|a| a == "--seed") {
        config.seed = Some(seed);
    }

    let mut sim = avian_core::Simulation::from_config(config.clone());
    // 6.2: no telemetry is generated unless `--output` is supplied. Without an
    // output target the exporter is inert — no frames collected, none written.
    let mut exporter = if output_path.is_some() {
        TelemetryExporter::new(usize::MAX)
    } else {
        TelemetryExporter::disabled()
    };

    // 3.4: output path carries the format extension (only set with --output).
    let ext = format.extension();
    let out_path = output_path.as_deref().map(|op| {
        if op.ends_with(".csv") || op.ends_with(".jsonl") {
            let trimmed = op.trim_end_matches(".csv").trim_end_matches(".jsonl");
            format!("{trimmed}.{ext}")
        } else {
            op.to_string()
        }
    });

    // Telemetry file is only opened when --output is supplied.
    if let Some(out) = &out_path {
        // Fix #8: Open telemetry file at startup — stream to disk, no data loss
        exporter
            .open_with_format(std::path::Path::new(out), format)
            .expect("Failed to open telemetry file");
        // 2.5: side-car event log for ground-truth annotations.
        let events_out = out.replace(&format!(".{ext}"), ".events.jsonl");
        exporter
            .open_event_log(std::path::Path::new(&events_out))
            .ok();
    }

    // 3.7: dataset metadata (schema authority) written up front; reward stats
    // and sim_frames are patched in at end of run.
    let mut metadata = TelemetryMetadata::new(
        seed,
        serde_json::to_value(&config).unwrap_or(serde_json::json!({})),
        30,
        [calibration::WORLD_WIDTH_M, calibration::WORLD_HEIGHT_M],
        events_file
            .clone()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|c| c.lines().map(|l| l.to_string()).collect())
            .unwrap_or_default(),
        0,
        None,
    );
    let meta_path = out_path
        .as_deref()
        .map(|out| out.replace(&format!(".{ext}"), ".metadata.json"));
    if let Some(mp) = &meta_path {
        let _ = write_metadata(std::path::Path::new(mp), &metadata);
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .ok();

    // 5.2: initial population/grains come from the scenario config.
    // Audit 5a item 2: when the scripted schedule is active, the initial
    // population is exactly SCRIPTED_START_AGENTS (the config's initial_agents
    // is ignored) and the per-tick growth check below drives everything after.
    let mut scripted_growth = ScriptedGrowth::default();
    if config.scripted_population {
        scripted_growth.spawn_start(&mut sim);
        println!(
            "Scripted population active: starting with {} agents, growing per schedule",
            avian_agent::scripted_population::SCRIPTED_START_AGENTS
        );
    } else {
        for _ in 0..config.initial_agents {
            let pos = avian_core::Simulation::random_free_point(
                sim.config.world_width,
                sim.config.world_height,
                &sim.obstacles,
                &mut sim.rng,
            )
            .unwrap_or_else(|| {
                nalgebra::Vector2::new(sim.config.world_width / 2.0, sim.config.world_height / 2.0)
            });
            let uid = sim.next_uid_str();
            spawn_agent(&mut sim.world, &mut sim.rng, pos, &mut sim.physics, uid);
        }
    }
    for _ in 0..config.initial_grains {
        let x = sim.rng.gen_range(2.0..sim.config.world_width - 2.0);
        let y = sim.rng.gen_range(2.0..sim.config.world_height - 2.0);
        spawn_grain(&mut sim, nalgebra::Vector2::new(x, y), 10);
    }

    // 2.5: inject pre-recorded events from a JSONL file (headless scenario
    // control). Each event lands at frame 0 of the run.
    let mut pending_events: Vec<Event> = Vec::new();
    if let Some(path) = &events_file {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                if let Ok(ev) = serde_json::from_str::<Event>(line) {
                    pending_events.push(ev.clone());
                    sim.inject_event(ev);
                }
            }
        }
    }
    // 5.2: the toml `event_schedule` injects at frame 0, same semantics.
    for ev in &config.event_schedule {
        pending_events.push(ev.clone());
        sim.inject_event(ev.clone());
    }

    // Fix #3: Headless mode — run simulation without any client
    if headless {
        println!(
            "Headless mode: running {} frames, output to {}",
            if frames_target == 0 {
                "unlimited".to_string()
            } else {
                frames_target.to_string()
            },
            out_path.as_deref().unwrap_or("(no telemetry file)")
        );
        let mut frame: u64 = 0;
        while running.load(Ordering::SeqCst) {
            if frames_target > 0 && frame >= frames_target {
                break;
            }
            sim.step(|s, dt| run_systems(s, dt, &mut exporter));
            frame += 1;
            // Audit 5a item 2: scripted growth is checked once per tick, keyed
            // on sim-time (frame × dt), so it stays correct at any speed.
            if config.scripted_population {
                let n = scripted_growth.tick(&mut sim);
                if n > 0 {
                    println!("Scripted population: +{n} agents at frame {frame}");
                }
            }
            if frame.is_multiple_of(1000) {
                println!(
                    "Frame {} — agents: {}, grains: {}, telemetry frames: {}",
                    frame,
                    sim.snapshot().agents.len(),
                    sim.snapshot().grains.len(),
                    exporter.frame_count()
                );
            }
        }
        println!(
            "Headless run complete. {} frames, {} telemetry frames written to {}",
            frame,
            exporter.frame_count(),
            out_path.as_deref().unwrap_or("(telemetry disabled)")
        );
        // 3.4/3.7: flush pending `next_fsm` frames + finalize metadata.
        exporter.finish();
        metadata.sim_frames = frame;
        metadata.reward_stats = exporter.reward_stats();
        if let Some(mp) = &meta_path {
            let _ = write_metadata(std::path::Path::new(mp), &metadata);
        }
        return;
    }

    // Fix #3: Interactive mode — accept multiple connections, don't die on disconnect
    let server = TcpListener::bind("127.0.0.1:8080").unwrap();
    println!("Serwer WebSocket uruchomiony na ws://127.0.0.1:8080");
    server.set_nonblocking(true).ok();

    let mut clients: Vec<WebSocket<TcpStream>> = Vec::new();
    let mut frame: u64 = 0;
    // 6.1: transport-level time controls (independent of the sim).
    let mut paused = false;
    let mut pending_step = false;
    let mut speed: f64 = 1.0;
    // 6.2: dashboard metrics are pushed every N frames (not every frame).
    let mut metrics_pending = true;
    // Audit 2 Task 1: wall-clock pacing for the outbound broadcast. Snapshot /
    // metrics / event-log messages are sent at most every BROADCAST_INTERVAL
    // of real time regardless of `speed`, which only drives how fast
    // `sim.step()` runs. Incoming client messages are handled every iteration.
    let mut last_broadcast = std::time::Instant::now();

    while running.load(Ordering::SeqCst) {
        // Accept new connections
        match server.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(true).ok();
                if let Ok(ws) = accept(stream) {
                    println!("Client connected ({} total)", clients.len() + 1);
                    clients.push(ws);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }

        // 6.1: step only while running (or on an explicit single-step). A
        // paused server still streams snapshots so the view stays live.
        let do_step = !paused || pending_step;
        pending_step = false;
        if do_step {
            sim.step(|s, dt| run_systems(s, dt, &mut exporter));
            frame += 1;
            // Audit 5a item 2: scripted growth check once per tick (sim-time).
            if config.scripted_population {
                let n = scripted_growth.tick(&mut sim);
                if n > 0 {
                    println!("Scripted population: +{n} agents at frame {frame}");
                }
            }
        }
        // 6.2: refresh the dashboard metrics every 100 sim frames.
        if frame.is_multiple_of(100) {
            metrics_pending = true;
        }

        let mut disconnected: Vec<usize> = Vec::new();

        // ---- Step 1: handle incoming client messages (EVERY iteration) ----
        // Transport control commands (pause/step/speed) and injected events
        // must be processed on every loop iteration, completely independent of
        // the broadcast pacing below, so the UI stays instantly responsive at
        // any speed setting.
        for (i, ws) in clients.iter_mut().enumerate() {
            // Read incoming messages
            loop {
                match ws.read() {
                    Ok(msg) => {
                        if let tungstenite::Message::Text(text) = msg {
                            if text.starts_with('{') {
                                // 6.1: transport control commands (pause/step/
                                // speed) — never injected into the sim.
                                if let Ok(ctrl) = serde_json::from_str::<serde_json::Value>(&text) {
                                    if let Some(cmd) = ctrl.get("command").and_then(|c| c.as_str())
                                    {
                                        match cmd {
                                            "pause" => paused = true,
                                            "resume" => paused = false,
                                            "step" => pending_step = true,
                                            "speed" => {
                                                if let Some(v) =
                                                    ctrl.get("value").and_then(|v| v.as_f64())
                                                {
                                                    speed = v.max(0.1);
                                                }
                                            }
                                            _ => {}
                                        }
                                        continue;
                                    }
                                }
                                // 2.5: JSON events from the RLHF controller
                                // (`{"event":"spawn_predator",...}`).
                                if let Ok(ev) = serde_json::from_str::<Event>(&text) {
                                    pending_events.push(ev.clone());
                                    sim.inject_event(ev);
                                }
                            } else if text.contains("spawn_grain") {
                                let parts: Vec<&str> = text.split(',').collect();
                                if parts.len() == 3 {
                                    let x = parts[1].parse().unwrap_or(10.0);
                                    let y = parts[2].parse().unwrap_or(10.0);
                                    pending_events.push(Event::SpawnGrain(
                                        avian_core::events::SpawnGrainRequest {
                                            pos: [x, y],
                                            count: 10,
                                        },
                                    ));
                                    spawn_grain(&mut sim, nalgebra::Vector2::new(x, y), 10);
                                }
                            }
                        }
                    }
                    Err(tungstenite::Error::Io(ref e))
                        if e.kind() == std::io::ErrorKind::WouldBlock =>
                    {
                        break
                    }
                    Err(_) => {
                        disconnected.push(i);
                        break;
                    }
                }
            }
        }

        // ---- Step 2: broadcast outgoing state (paced to ~60 Hz real time) ----
        // Audit 2 Task 1: the snapshot + metrics + event-log payloads are only
        // built and sent when at least BROADCAST_INTERVAL of wall-clock time
        // has elapsed since the last broadcast. This is metadata/network pacing
        // only — it never influences simulation state, dt, or the RNG.
        let now = std::time::Instant::now();
        if now.duration_since(last_broadcast) >= BROADCAST_INTERVAL {
            last_broadcast = now;

            // Build each payload ONCE per broadcast tick, not once per client.
            let snap = sim.snapshot();

            // Audit 4 §9.6: ONE coalesced message per broadcast tick — snapshot,
            // event_log, and metrics ride in a single JSON object / single
            // ws.send per client (down from up to 3 sends). Cuts per-tick I/O
            // call count 3x and removes the partial-send stall window on slow
            // connections.
            let event_log = if !pending_events.is_empty() {
                Some(serde_json::json!({
                    "frame": frame,
                    "events": pending_events
                        .iter()
                        .map(|e| serde_json::to_value(e).unwrap_or(serde_json::Value::Null))
                        .collect::<Vec<_>>(),
                }))
            } else {
                None
            };
            let metrics = if metrics_pending {
                Some(compute_metrics(
                    &snap,
                    sim.predator_kills,
                    sim.grains_consumed,
                    &sim.death_ages,
                    sim.config.world_width,
                    sim.config.world_height,
                ))
            } else {
                None
            };
            let text = serde_json::json!({
                "snapshot": snap,
                "event_log": event_log,
                "metrics": metrics,
            })
            .to_string();

            for (i, ws) in clients.iter_mut().enumerate() {
                // WouldBlock → skip this client this broadcast (try again next
                // time); any genuine error → disconnect.
                if let Err(e) = ws.send(tungstenite::Message::Text(text.clone())) {
                    if send_error_is_fatal(&e) {
                        disconnected.push(i);
                    }
                }
            }

            // Pending state is only cleared once it has actually been included
            // in a broadcast — never unconditionally on every loop iteration,
            // otherwise metrics/event-log messages would be silently dropped
            // whenever the sim runs faster than the broadcast rate.
            pending_events.clear();
            metrics_pending = false;
        }

        // Remove disconnected clients (in reverse order to keep indices valid)
        disconnected.sort_unstable();
        disconnected.dedup();
        for i in disconnected.iter().rev() {
            println!("Client disconnected ({} remaining)", clients.len() - 1);
            clients.remove(*i);
        }

        // 5.2/6.1: interactive pacing = 16 ms / (time_scale × speed). Speed
        // 1×/10×/100× from the viewer's time controls shortens the sleep.
        // Audit 4 §9.3: this is the server-side 60fps cap — the loop never
        // spins faster than ~60 Hz regardless of client count. Do NOT remove.
        std::thread::sleep(std::time::Duration::from_secs_f64(
            16.0 / 1000.0 / sim.config.time_scale / speed,
        ));
    }

    println!(
        "Shutting down. {} telemetry frames written to {}",
        exporter.frame_count(),
        out_path.as_deref().unwrap_or("(telemetry disabled)")
    );
    exporter.finish();
    metadata.sim_frames = frame;
    metadata.reward_stats = exporter.reward_stats();
    if let Some(mp) = &meta_path {
        let _ = write_metadata(std::path::Path::new(mp), &metadata);
    }
}
