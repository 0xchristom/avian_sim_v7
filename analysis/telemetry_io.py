"""Shared telemetry loader for the 6.3 analysis scripts (3.4/3.7 format).

Reads the streaming telemetry written by `TelemetryExporter` — CSV (default)
or JSONL (`--format jsonl`). Also loads `calibration_export.json` so every
script reads the compiled constants instead of a second hardcoded copy.

Telemetry CSV columns:
  time_us,frame,uid,reward,reward_grain,reward_flocking,reward_starvation,
  reward_captured,reward_flee_success,alarm,sick,fsm,next_fsm,events,obs

obs_v1 (128 dims) — frozen layout, see avian_telemetry/src/rlhf.rs:
  [0..2]    pos (normalized to world size)
  [2..4]    heading (sin + cos)
  [4]       velocity magnitude
  [5]       energy (normalized)
  [6]       hunger
  [7]       age (normalized to WILD_MAX_LIFESPAN_YEARS)
  [8]       vitality
  [9]       light_level
  [10..16]  nearest 3 grains rel pos (2 each)
  [16..37]  7 neighbors rel pos + dist (3 each)
  [37..43]  predator rel pos + threat + alarm_flag (+ 2 reserved)
  [43..51]  memory locations (all zero until it ships)
  [51..127] reserved — all zero
"""
from __future__ import annotations

import csv
import json
from dataclasses import dataclass
from pathlib import Path

#: Directory containing this module — sibling to calibration_export.json.
_ANALYSIS_DIR = Path(__file__).resolve().parent


def load_calibration() -> dict:
    """Load the shared constants exported from calibration.rs.

    Raises FileNotFoundError with a regenerate hint if missing.
    """
    path = _ANALYSIS_DIR / "calibration_export.json"
    if not path.exists():
        raise FileNotFoundError(
            f"{path} missing — regenerate it with: "
            "cargo run -p avian_core --bin calibration_export"
        )
    with path.open("r", encoding="utf-8") as f:
        return json.load(f)


@dataclass
class Frame:
    time_us: int
    frame: int
    uid: str
    reward: float
    reward_grain: float
    reward_flocking: float
    reward_starvation: float
    reward_captured: float
    reward_flee_success: float
    alarm: bool
    sick: bool
    fsm: str
    next_fsm: str
    events: list[str]
    obs: list[float]


def _parse_obs(raw: str) -> list[float]:
    return [float(v) for v in raw.split(";")] if raw else []


def read_telemetry(path: str | Path) -> list[Frame]:
    """Read a telemetry file (CSV or JSONL, auto-detected by extension)."""
    path = Path(path)
    if not path.exists():
        raise FileNotFoundError(f"telemetry file not found: {path}")

    frames: list[Frame] = []
    if path.suffix.lower() == ".jsonl":
        with path.open("r", encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                d = json.loads(line)
                frames.append(
                    Frame(
                        time_us=d["time_us"],
                        frame=d["frame"],
                        uid=d["uid"],
                        reward=d["reward"],
                        reward_grain=d["reward_grain"],
                        reward_flocking=d["reward_flocking"],
                        reward_starvation=d["reward_starvation"],
                        reward_captured=d["reward_captured"],
                        reward_flee_success=d["reward_flee_success"],
                        alarm=d["alarm_triggered"],
                        sick=d["sick"],
                        fsm=d["fsm"],
                        next_fsm=d.get("next_fsm", ""),
                        events=list(d.get("event_labels", [])),
                        obs=list(d.get("obs", [])),
                    )
                )
        return frames

    with path.open("r", encoding="utf-8-sig") as f:
        reader = csv.DictReader(f)
        for row in reader:
            frames.append(
                Frame(
                    time_us=int(row["time_us"]),
                    frame=int(row["frame"]),
                    uid=row["uid"],
                    reward=float(row["reward"]),
                    reward_grain=float(row["reward_grain"]),
                    reward_flocking=float(row["reward_flocking"]),
                    reward_starvation=float(row["reward_starvation"]),
                    reward_captured=float(row["reward_captured"]),
                    reward_flee_success=float(row["reward_flee_success"]),
                    alarm=row["alarm"] == "1",
                    sick=row["sick"] == "1",
                    fsm=row["fsm"],
                    next_fsm=row.get("next_fsm", ""),
                    events=[e for e in row.get("events", "").split(";") if e],
                    obs=_parse_obs(row.get("obs", "")),
                )
            )
    return frames


def agent_positions(frame: Frame, calib: dict) -> tuple[float, float]:
    """World-space (m) position from obs[0..2] (normalized to world size)."""
    if len(frame.obs) < 2:
        return (0.0, 0.0)
    return (
        frame.obs[0] * calib["world_width_m"],
        frame.obs[1] * calib["world_height_m"],
    )


def key_by_uid(frames: list[Frame]) -> dict[str, list[Frame]]:
    """Group frames per agent, preserving arrival order."""
    by_uid: dict[str, list[Frame]] = {}
    for fr in frames:
        by_uid.setdefault(fr.uid, []).append(fr)
    return by_uid
