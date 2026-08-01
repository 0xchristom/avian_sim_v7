use avian_core::{Simulation, SimulationConfig};
use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::{run_systems, spawn_grain};
use avian_telemetry::exporter::{TelemetryExporter, TelemetryFrame};
use std::net::TcpListener;
use tungstenite::accept;

fn main() {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    let mut exporter = TelemetryExporter::new(10000);
    
    for _ in 0..30 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        spawn_agent(&mut sim.world, &mut sim.rng, nalgebra::Vector2::new(x, y), &mut sim.physics);
    }

    for _ in 0..15 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        spawn_grain(&mut sim, nalgebra::Vector2::new(x, y), 10);
    }

    let server = TcpListener::bind("127.0.0.1:8080").unwrap();
    println!("Serwer WebSocket uruchomiony na ws://127.0.0.1:8080");

    for stream in server.incoming() {
        let mut websocket = accept(stream.unwrap()).unwrap();
        loop {
            sim.step(|s, dt| run_systems(s, dt, &mut exporter));

            let snap = sim.snapshot();
            let json = serde_json::to_string(&snap).unwrap();
            
            // Odczyt komend z frontendu (Ticket 10)
            if let Ok(msg) = websocket.read() {
                if let tungstenite::Message::Text(text) = msg {
                    if text.contains("spawn_grain") {
                        let parts: Vec<&str> = text.split(',').collect();
                        if parts.len() == 3 {
                            let x = parts[1].parse().unwrap_or(10.0);
                            let y = parts[2].parse().unwrap_or(10.0);
                            spawn_grain(&mut sim, nalgebra::Vector2::new(x, y), 10);
                        }
                    }
                }
            }

            if websocket.send(tungstenite::Message::Text(json)).is_err() { break; }
            
            // Zapisz dane RLHF
            exporter.push(TelemetryFrame { time_us: snap.time_us, frame: snap.frame });
            if snap.frame % 1000 == 0 {
                let _ = exporter.flush_to_parquet(std::path::Path::new("dataset.parquet"));
            }
            
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }
}