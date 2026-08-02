#!/usr/bin/env python3
"""6.3: split a continuous telemetry stream into episodes for RLHF.

An RLHF episode is a bounded segment of an agent's trajectory with a clear
start and outcome. This script slices each agent's per-uid frame stream on the
natural episode boundaries:
  - agent captured (reward_captured != 0) → episode ends with a capture outcome
  - agent flees successfully (reward_flee_success != 0) → ends with a flee-outcome
  - a `next_fsm` transition INTO a terminal state (Sick after fleeing, or the
    final frame of a run) → ends the episode

Output: JSONL with one episode per line, each carrying the agent uid, start/
end frame, length, total + component rewards, and the slice of frames (compact
form). Writes `episodes.jsonl` by default.

Usage:
  python analysis/extract_episodes.py telemetry.csv
  python analysis/extract_episodes.py telemetry.csv --out episodes.jsonl --min-len 10
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from telemetry_io import Frame, key_by_uid, read_telemetry


def episode_outcome(frames: list[Frame]) -> str:
    """Label the episode by its terminal event."""
    for fr in reversed(frames):
        if fr.reward_captured != 0.0:
            return "captured"
        if fr.reward_flee_success != 0.0:
            return "flee_success"
    return "timeout"


def split_episodes(frames: list[Frame], min_len: int) -> list[dict]:
    """Split one agent's frames into episodes on capture/flee boundaries."""
    episodes: list[dict] = []
    start = 0
    for i, fr in enumerate(frames):
        if fr.reward_captured != 0.0 or fr.reward_flee_success != 0.0:
            seg = frames[start : i + 1]
            if len(seg) >= min_len:
                episodes.append(serialize_episode(seg))
            start = i + 1
    tail = frames[start:]
    if tail and len(tail) >= min_len:
        episodes.append(serialize_episode(tail))
    return episodes


def serialize_episode(seg: list[Frame]) -> dict:
    uid = seg[0].uid
    return {
        "uid": uid,
        "start_frame": seg[0].frame,
        "end_frame": seg[-1].frame,
        "length": len(seg),
        "outcome": episode_outcome(seg),
        "reward": round(sum(f.reward for f in seg), 6),
        "reward_grain": round(sum(f.reward_grain for f in seg), 6),
        "reward_flocking": round(sum(f.reward_flocking for f in seg), 6),
        "reward_starvation": round(sum(f.reward_starvation for f in seg), 6),
        "reward_captured": round(sum(f.reward_captured for f in seg), 6),
        "reward_flee_success": round(sum(f.reward_flee_success for f in seg), 6),
        "n_captured": sum(1 for f in seg if f.reward_captured != 0.0),
        "n_flee_success": sum(1 for f in seg if f.reward_flee_success != 0.0),
        "frames": [
            {
                "frame": f.frame,
                "fsm": f.fsm,
                "next_fsm": f.next_fsm,
                "reward": f.reward,
                "obs": f.obs,
            }
            for f in seg
        ],
    }


def main() -> None:
    ap = argparse.ArgumentParser(description="Split telemetry into RLHF episodes.")
    ap.add_argument("telemetry", type=Path, help="telemetry CSV/JSONL file")
    ap.add_argument("--out", type=Path, default=Path("episodes.jsonl"))
    ap.add_argument("--min-len", type=int, default=10, help="drop episodes shorter than this")
    args = ap.parse_args()

    frames = read_telemetry(args.telemetry)
    by_uid = key_by_uid(frames)
    print(f"{len(frames)} frames, {len(by_uid)} agents")

    episodes: list[dict] = []
    for uid, agent_frames in by_uid.items():
        eps = split_episodes(agent_frames, args.min_len)
        episodes.extend(eps)
        for ep in eps:
            print(
                f"  {ep['uid']:>10s}  f{ep['start_frame']:>6d}-{ep['end_frame']:>6d}  "
                f"len={ep['length']:>5d}  outcome={ep['outcome']:<12s}  reward={ep['reward']:+8.3f}"
            )

    with args.out.open("w", encoding="utf-8") as f:
        for ep in episodes:
            f.write(json.dumps(ep) + "\n")
    print(f"wrote {len(episodes)} episodes -> {args.out}")


if __name__ == "__main__":
    main()
