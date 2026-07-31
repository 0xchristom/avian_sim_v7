use avian_core::{Simulation, SimulationConfig};
use avian_agent::gerontology::spawn_agent;
use avian_agent::systems::run_systems;
use std::net::TcpListener;
use tungstenite::accept;

fn main() {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    for _ in 0..30 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        spawn_agent(&mut sim.world, &mut sim.rng, nalgebra::Vector2::new(x, y));
    }

    let server = TcpListener::bind("127.0.0.1:8080").unwrap();
    println!("Serwer WebSocket uruchomiony na ws://127.0.0.1:8080");

    for stream in server.incoming() {
        let mut websocket = accept(stream.unwrap()).unwrap();
        loop {
            sim.step();
            run_systems(&mut sim); // Logika wykonuje się w ECS, nie w I/O

            let snap = sim.snapshot();
            let json = serde_json::to_string(&snap).unwrap();
            if websocket.send(tungstenite::Message::Text(json)).is_err() { break; }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }
}