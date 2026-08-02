use avian_core::calibration;
use avian_core::events::Event;
use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::{run_systems, spawn_grain};
use avian_telemetry::{Format, TelemetryExporter, TelemetryMetadata, write_metadata};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tungstenite::accept;
use std::net::TcpStream;
use tungstenite::WebSocket;

fn main() {
    // Parse CLI args: --headless [--frames N] [--output path] [--events-file path] [--seed N]
    let args: Vec<String> = std::env::args().collect();
    let headless = args.iter().any(|a| a == "--headless");
    // 4.3: opt-in urban map (buildings/trees/water + line-of-sight occlusion).
    let urban = args.iter().any(|a| a == "--urban");
    // 4.4: opt-in stochastic weather scheduler (Clear/Rain/Wind/Heat).
    let weather = args.iter().any(|a| a == "--weather");
    let frames_target: u64 = args.iter()
        .position(|a| a == "--frames")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0); // 0 = unlimited
    // Per-run seed — metadata.json is designed to track this per run.
    let seed: u64 = args.iter()
        .position(|a| a == "--seed")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(42);
    // Telemetry file output is opt-in via `--output <path>`. Without it the
    // server runs without writing any dataset, so it never grows a dataset.csv.
    let output_path = args.iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());
    let events_file = args.iter()
        .position(|a| a == "--events-file")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());
    // 3.4: export format. `--format jsonl` for the lossless debug format.
    let format = args.iter()
        .position(|a| a == "--format")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| Format::from_str(s))
        .unwrap_or(Format::Csv);
    // 5.2: base scenario from a `simulation.toml` file. CLI flags below override
    // the file on collision (explicit command line wins over a file default).
    let config_path = args.iter()
        .position(|a| a == "--config")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.to_string());

    let mut config = match &config_path {
        Some(path) => match avian_core::SimulationConfig::from_file(path) {
            Ok(c) => { println!("Scenario config loaded from {path}"); c }
            Err(e) => {
                eprintln!("Failed to read config file {path}: {e}");
                std::process::exit(1);
            }
        },
        None => avian_core::SimulationConfig::default(),
    };
    // CLI overrides (win over the file).
    config.urban_obstacles |= urban;
    config.weather_enabled |= weather;
    if args.iter().any(|a| a == "--seed") {
        config.seed = Some(seed);
    }

    let mut sim = avian_core::Simulation::from_config(config.clone());
    let mut exporter = TelemetryExporter::new(usize::MAX);

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
        exporter.open_with_format(std::path::Path::new(out), format).expect("Failed to open telemetry file");
        // 2.5: side-car event log for ground-truth annotations.
        let events_out = out.replace(&format!(".{ext}"), ".events.jsonl");
        exporter.open_event_log(std::path::Path::new(&events_out)).ok();
    }

    // 3.7: dataset metadata (schema authority) written up front; reward stats
    // and sim_frames are patched in at end of run.
    let mut metadata = TelemetryMetadata::new(
        seed,
        serde_json::to_value(&config).unwrap_or(serde_json::json!({})),
        30,
        [calibration::WORLD_WIDTH_M, calibration::WORLD_HEIGHT_M],
        events_file.clone()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|c| c.lines().map(|l| l.to_string()).collect())
            .unwrap_or_default(),
        0,
        None,
    );
    let meta_path = out_path.as_deref().map(|out| out.replace(&format!(".{ext}"), ".metadata.json"));
    if let Some(mp) = &meta_path {
        let _ = write_metadata(std::path::Path::new(mp), &metadata);
    }

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || { r.store(false, Ordering::SeqCst); }).ok();

    // 5.2: initial population/grains come from the scenario config.
    for _ in 0..config.initial_agents {
        let pos = avian_core::Simulation::random_free_point(
            sim.config.world_width,
            sim.config.world_height,
            &sim.obstacles,
            &mut sim.rng,
        );
        let uid = sim.next_uid_str();
        spawn_agent(&mut sim.world, &mut sim.rng, pos, &mut sim.physics, uid);
    }
    for _ in 0..config.initial_grains {
        let x = sim.rng.gen_range(2.0..sim.config.world_width - 2.0);
        let y = sim.rng.gen_range(2.0..sim.config.world_height - 2.0);
        spawn_grain(&mut sim, nalgebra::Vector2::new(x, y), 10);
    }

    // 2.5: inject pre-recorded events from a JSONL file (headless scenario
    // control). Each event lands at frame 0 of the run.
    if let Some(path) = &events_file {
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                if let Ok(ev) = serde_json::from_str::<Event>(line) {
                    sim.inject_event(ev);
                }
            }
        }
    }
    // 5.2: the toml `event_schedule` injects at frame 0, same semantics.
    for ev in &config.event_schedule {
        sim.inject_event(ev.clone());
    }

    // Fix #3: Headless mode — run simulation without any client
    if headless {
        println!("Headless mode: running {} frames, output to {}",
            if frames_target == 0 { "unlimited".to_string() } else { frames_target.to_string() },
            out_path.as_deref().unwrap_or("(no telemetry file)"));
        let mut frame: u64 = 0;
        while running.load(Ordering::SeqCst) {
            if frames_target > 0 && frame >= frames_target { break; }
            sim.step(|s, dt| run_systems(s, dt, &mut exporter));
            frame += 1;
            if frame % 1000 == 0 {
                println!("Frame {} — agents: {}, grains: {}, telemetry frames: {}",
                    frame, sim.snapshot().agents.len(), sim.snapshot().grains.len(), exporter.frame_count());
            }
        }
        println!("Headless run complete. {} frames, {} telemetry frames written to {}",
            frame, exporter.frame_count(), out_path.as_deref().unwrap_or("(no telemetry file)"));
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

    while running.load(Ordering::SeqCst) {
        // Accept new connections
        match server.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(true).ok();
                match accept(stream) {
                    Ok(ws) => {
                        println!("Client connected ({} total)", clients.len() + 1);
                        clients.push(ws);
                    }
                    Err(_) => {}
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }

        // Run one simulation step
        sim.step(|s, dt| run_systems(s, dt, &mut exporter));
        frame += 1;
        let snap = sim.snapshot();
        let json = serde_json::to_string(&snap).unwrap();

        // Handle client messages and send snapshots
        let mut disconnected: Vec<usize> = Vec::new();
        for (i, ws) in clients.iter_mut().enumerate() {
            // Read incoming messages
            loop {
                match ws.read() {
                    Ok(msg) => {
                        if let tungstenite::Message::Text(text) = msg {
                            // 2.5: JSON events from the RLHF controller
                            // (`{"event":"spawn_predator",...}`). Backward
                            // compat with the old "spawn_grain,x,y" text form.
                            if text.starts_with('{') {
                                if let Ok(ev) = serde_json::from_str::<Event>(&text) {
                                    sim.inject_event(ev);
                                }
                            } else if text.contains("spawn_grain") {
                                let parts: Vec<&str> = text.split(',').collect();
                                if parts.len() == 3 {
                                    let x = parts[1].parse().unwrap_or(10.0);
                                    let y = parts[2].parse().unwrap_or(10.0);
                                    spawn_grain(&mut sim, nalgebra::Vector2::new(x, y), 10);
                                }
                            }
                        }
                    }
                    Err(tungstenite::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => { disconnected.push(i); break; }
                }
            }

            // Send snapshot
            if ws.send(tungstenite::Message::Text(json.clone())).is_err() {
                disconnected.push(i);
            }
        }

        // Remove disconnected clients (in reverse order to keep indices valid)
        disconnected.sort_unstable();
        disconnected.dedup();
        for i in disconnected.iter().rev() {
            println!("Client disconnected ({} remaining)", clients.len() - 1);
            clients.remove(*i);
        }

        // 5.2: interactive pacing = 16 ms real time / time_scale (1× ≈ 60fps).
        std::thread::sleep(std::time::Duration::from_secs_f64(
            16.0 / 1000.0 / sim.config.time_scale,
        ));
    }

    println!("Shutting down. {} telemetry frames written to {}", exporter.frame_count(), out_path.as_deref().unwrap_or("(no telemetry file)"));
    exporter.finish();
    metadata.sim_frames = frame;
    metadata.reward_stats = exporter.reward_stats();
    if let Some(mp) = &meta_path {
        let _ = write_metadata(std::path::Path::new(mp), &metadata);
    }
}
