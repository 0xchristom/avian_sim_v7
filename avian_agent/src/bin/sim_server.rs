use avian_core::{Simulation, SimulationConfig};
use avian_core::components::{Position, Heading};
use avian_agent::gerontology::spawn_agent;
use avian_core::rng::SimRng;
use std::net::TcpListener;
use tungstenite::accept;

fn main() {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    let mut rng = SimRng::from_seed(42);
    
    // Wykorzystujemy naszą gotową funkcję z biblioteki
    for i in 0..20 {
        let pos = nalgebra::Vector2::new(i as f64 * 1.5, 10.0);
        spawn_agent(&mut sim.world, &mut rng, pos);
    }

    let server = TcpListener::bind("127.0.0.1:8080").unwrap();
    println!("Serwer WebSocket uruchomiony na ws://127.0.0.1:8080");

    for stream in server.incoming() {
        let mut websocket = accept(stream.unwrap()).unwrap();
        println!("Frontend połączony!");
        
        loop {
            sim.step();
            
            // Prosta pętla ruchu na potrzeby testu wizualnego
            for (_id, (pos, head)) in sim.world.query::<(&mut Position, &mut Heading)>().iter() {
                pos.0.x += head.0.cos() * 0.05;
                pos.0.y += head.0.sin() * 0.05;
                if pos.0.x > 32.0 || pos.0.x < 0.0 { head.0 += std::f64::consts::PI / 2.0; }
                if pos.0.y > 21.0 || pos.0.y < 0.0 { head.0 += std::f64::consts::PI / 2.0; }
            }

            let snap = sim.snapshot();
            let json = serde_json::to_string(&snap).unwrap();
            
            if websocket.send(tungstenite::Message::Text(json)).is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(16)); // ~60 FPS
        }
        println!("Frontend rozłączony. Czekam na nowe połączenie...");
    }
}