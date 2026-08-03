//! Event Injection API (2.5) — RLHF environment control.
//!
//! Wire format: internally-tagged enum, so each variant serializes as
//! `{"event":"spawn_grain","pos":[x,y],"count":N}` etc. This is option (b)
//! from the plan: variants are newtypes wrapping named-field request structs,
//! which makes the internally-tagged derive viable. The exact JSON shape is
//! pinned by a unit test in this module.
//!
//! All agent/predator-referencing variants use the stable `EntityUid` string
//! (3.3) — never the raw `hecs::Entity`, which is meaningless across a
//! network/file boundary.

use crate::components::Weather;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpawnGrainRequest {
    pub pos: [f64; 2],
    pub count: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpawnPredatorRequest {
    pub pos: [f64; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemovePredatorRequest {
    pub uid: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetWeatherRequest {
    pub weather: Weather,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TeleportAgentRequest {
    pub uid: String,
    pub pos: [f64; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KillAgentRequest {
    pub uid: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    SpawnGrain(SpawnGrainRequest),
    SpawnPredator(SpawnPredatorRequest),
    RemovePredator(RemovePredatorRequest),
    SetWeather(SetWeatherRequest),
    TeleportAgent(TeleportAgentRequest),
    KillAgent(KillAgentRequest),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_json_shape() {
        let ev = Event::SpawnGrain(SpawnGrainRequest {
            pos: [5.0, 6.0],
            count: 10,
        });
        let json = serde_json::to_string(&ev).unwrap();
        assert_eq!(
            json, r#"{"event":"spawn_grain","pos":[5.0,6.0],"count":10}"#,
            "wire shape changed — downstream parsers break"
        );
    }

    #[test]
    fn test_event_roundtrip_all_variants() {
        let events = vec![
            Event::SpawnGrain(SpawnGrainRequest {
                pos: [1.0, 2.0],
                count: 3,
            }),
            Event::SpawnPredator(SpawnPredatorRequest { pos: [4.0, 5.0] }),
            Event::RemovePredator(RemovePredatorRequest {
                uid: "A0001-000001".into(),
            }),
            Event::SetWeather(SetWeatherRequest {
                weather: Weather::Rain,
            }),
            Event::TeleportAgent(TeleportAgentRequest {
                uid: "A0001-000002".into(),
                pos: [9.0, 8.0],
            }),
            Event::KillAgent(KillAgentRequest {
                uid: "A0001-000003".into(),
            }),
        ];
        for ev in events {
            let json = serde_json::to_string(&ev).unwrap();
            let back: Event = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&back).unwrap();
            assert_eq!(json, json2, "roundtrip mismatch for {:?}", ev);
        }
    }
}
