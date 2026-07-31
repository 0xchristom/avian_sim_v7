use avian_core::{Simulation, SimulationConfig};
use avian_core::components::{Position, Heading, Velocity, Metabolism};
use avian_agent::gerontology::spawn_agent;
use avian_agent::metabolism::metabolism_system;
use avian_agent::search::{next_step, SearchMode};
use std::net::TcpListener;
use tungstenite::accept;
use std::collections::HashMap;
use std::time::{Instant, Duration};

fn main() {
    let mut sim = Simulation::new(42, SimulationConfig::default());
    let mut levy_steps: HashMap<hecs::Entity, f64> = HashMap::new();

    // Spawn 30 agentów z wariancją
    for _ in 0..30 {
        let x = sim.rng.gen_range(2.0..30.0);
        let y = sim.rng.gen_range(2.0..19.0);
        spawn_agent(&mut sim.world, &mut sim.rng, nalgebra::Vector2::new(x, y));
    }

    let server = TcpListener::bind("127.0.0.1:8080").unwrap();
    println!("Serwer WebSocket: ws://127.0.0.1:8080");

    for stream in server.incoming() {
        let mut websocket = accept(stream.unwrap()).unwrap();
        println!("Frontend połączony!");

        let mut last_time = Instant::now();
        let dt = sim.config.dt;
        let mut accumulator = 0.0;

        loop {
            let now = Instant::now();
            let frame_time = (now - last_time).as_secs_f64();
            last_time = now;
            accumulator += frame_time;

            // Fixed timestep: wykonuj tyle ticków, ile się zmieści
            while accumulator >= dt {
                sim.step();
                
                // Tymczasowa logika ruchu (do przeniesienia do core w przyszłości)
                metabolism_system(&mut sim.world, &sim.time);
                
                for (id, (pos, head, vel, meta)) in sim.world.query::<(&mut Position, &mut Heading, &mut Velocity, &Metabolism)>().iter() {
                    if meta.energy_kj < 2.0 {
                        vel.0 = nalgebra::Vector2::zeros();
                    } else {
                        // Wariancja prędkości zależna od masy i energii
                        let speed_factor = (meta.energy_kj / 60.0).min(1.0).max(0.2);
                        let base_speed = 0.8 + sim.rng.gen_range(0.0..0.6); // 0.8-1.4 m/s
                        let speed = base_speed * speed_factor;
                        
                        let remaining = levy_steps.entry(id).or_insert(0.0);
                        if *remaining <= 0.0 {
                            let (dist, new_head) = next_step(SearchMode::Levy, head.0, &mut sim.rng);
                            head.0 = new_head;
                            *remaining = dist.min(5.0);
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

                accumulator -= dt;
            }

            let snap = sim.snapshot();
            let json = serde_json::to_string(&snap).unwrap();

            if websocket.send(tungstenite::Message::Text(json)).is_err() {
                break;
            }
            
            // Krótkie uśpienie, ale nie jako mechanizm timingu
            std::thread::sleep(Duration::from_millis(1));
        }
        println!("Frontend rozłączony.");
    }
}