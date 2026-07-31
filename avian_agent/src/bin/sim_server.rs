use avian_core::{Simulation, SimulationConfig};
use avian_core::components::{Position, Heading, Velocity, Mass, Age, Metabolism};
use avian_agent::gerontology::spawn_agent;
use avian_agent::metabolism::metabolism_system;
use avian_agent::search::{next_step, SearchMode};
use avian_core::rng::SimRng;
use std::net::TcpListener;
use tungstenite::accept;
use rand::Rng;

fn main() {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    
    // Spawn 20 agentów w LOSOWYCH pozycjach na całej mapie
    for _ in 0..20 {
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
            
            // Uruchamiamy system metabolizmu (spadek energii, głód)
            metabolism_system(&mut sim.world, &sim.time);
            
            let dt = sim.config.dt;
            
            // Pętla fizyki i kognicji (Lévy Walk)
            for (_id, (pos, head, vel, meta)) in sim.world.query::<(&mut Position, &mut Heading, &mut Velocity, &Metabolism)>().iter() {
                // Jeśli energia krytycznie niska, ptak się zatrzymuje (REST)
                if meta.energy_kj < 5.0 {
                    vel.0 = nalgebra::Vector2::zeros();
                } else {
                    // Wybieramy nowy kierunek i dystans na podstawie Lévy Walk
                    let (dist, new_head) = next_step(SearchMode::Levy, head.0, &mut sim.rng);
                    head.0 = new_head;
                    
                    // Prędkość zależna od wylosowanego dystansu Lévy'ego
                    let speed = 0.5 + (dist % 2.0); 
                    vel.0 = nalgebra::Vector2::new(speed * head.0.cos(), speed * head.0.sin());
                }
                
                // Aktualizacja pozycji
                pos.0 += vel.0 * dt;
                
                // Realistyczne odbijanie się od ścian (z zachowaniem kąta)
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
            std::thread::sleep(std::time::Duration::from_millis(33)); // ~30 FPS
        }
        println!("Frontend rozłączony. Czekam na nowe połączenie...");
    }
}