use avian_core::{Simulation, SimulationConfig};
use avian_core::components::{Position, Heading, Velocity, Metabolism};
use avian_agent::gerontology::spawn_agent;
use avian_agent::metabolism::metabolism_system;
use avian_agent::search::{next_step, SearchMode};
use std::net::TcpListener;
use tungstenite::accept;
use std::collections::HashMap;

fn main() {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    let mut levy_steps: HashMap<hecs::Entity, f64> = HashMap::new();
    
    // Spawn 30 agentów
    for _ in 0..30 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        spawn_agent(&mut sim.world, &mut sim.rng, nalgebra::Vector2::new(x, y));
    }

    let server = TcpListener::bind("127.0.0.1:8080").unwrap();
    println!("Serwer WebSocket uruchomiony na ws://127.0.0.1:8080");

    for stream in server.incoming() {
        let mut websocket = accept(stream.unwrap()).unwrap();
        println!("Frontend połączony!");
        
        loop {
            sim.step();
            metabolism_system(&mut sim.world, &sim.time);
            
            let dt = sim.config.dt;
            
            for (id, (pos, head, vel, meta)) in sim.world.query::<(&mut Position, &mut Heading, &mut Velocity, &Metabolism)>().iter() {
                if meta.energy_kj < 5.0 {
                    vel.0 = nalgebra::Vector2::zeros();
                } else {
                    let speed = 1.0;
                    let remaining = levy_steps.entry(id).or_insert(0.0);
                    
                    // TUTAJ NAPRAWIAMY DRGANIA: Zmieniamy kierunek TYLKO gdy dystans wyczerpany
                    if *remaining <= 0.0 {
                        let (dist, new_head) = next_step(SearchMode::Levy, head.0, &mut sim.rng);
                        head.0 = new_head;
                        *remaining = dist.min(5.0); // Ograniczamy do max 5m jednorazowo
                    } else {
                        *remaining -= speed * dt;
                    }
                    
                    vel.0 = nalgebra::Vector2::new(speed * head.0.cos(), speed * head.0.sin());
                }
                
                pos.0 += vel.0 * dt;
                
                // Odbicia od ścian
                if pos.0.x < 0.5 { pos.0.x = 0.5; head.0 = std::f64::consts::PI - head.0; }
                if pos.0.x > 31.5 { pos.0.x = 31.5; head.0 = std::f64::consts::PI - head.0; }
                if pos.0.y < 0.5 { pos.0.y = 0.5; head.0 = -head.0; }
                if pos.0.y > 20.5 { pos.0.y = 20.5; head.0 = -head.0; }
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