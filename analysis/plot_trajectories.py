#!/usr/bin/env python3
"""6.3: plot agent trajectories over time from a telemetry export.

Reads positions from obs[0..2] (normalized) and plots each agent's path in
world-space meters. Requires matplotlib (optional at CI — degrades to a
summary table if unavailable).

Usage:
  python analysis/plot_trajectories.py telemetry.csv
  python analysis/plot_trajectories.py telemetry.csv --out paths.png --max-agents 20
"""
from __future__ import annotations

import argparse
from pathlib import Path

from telemetry_io import agent_positions, key_by_uid, load_calibration, read_telemetry


def main() -> None:
    ap = argparse.ArgumentParser(description="Plot agent trajectories from telemetry.")
    ap.add_argument("telemetry", type=Path, help="telemetry CSV/JSONL file")
    ap.add_argument("--out", type=Path, default=Path("trajectories.png"), help="output PNG")
    ap.add_argument("--max-agents", type=int, default=20, help="cap on agents plotted")
    args = ap.parse_args()

    calib = load_calibration()
    frames = read_telemetry(args.telemetry)
    by_uid = key_by_uid(frames)
    print(f"{len(frames)} frames, {len(by_uid)} agents")

    summary = []
    for uid, agent_frames in sorted(by_uid.items())[: args.max_agents]:
        pts = [agent_positions(fr, calib) for fr in agent_frames]
        summary.append((uid, len(pts), pts[0], pts[-1]))
        print(
            f"{uid:>12s}  {len(pts):>5d} frames  "
            f"({pts[0][0]:5.1f},{pts[0][1]:5.1f}) -> ({pts[-1][0]:5.1f},{pts[-1][1]:5.1f})"
        )

    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        print("matplotlib not installed — wrote summary table only; run: pip install matplotlib")
        return

    fig, ax = plt.subplots(figsize=(9, 6))
    for uid, agent_frames in sorted(by_uid.items())[: args.max_agents]:
        xs = [agent_positions(fr, calib)[0] for fr in agent_frames]
        ys = [agent_positions(fr, calib)[1] for fr in agent_frames]
        ax.plot(xs, ys, linewidth=0.8, label=uid)
        ax.scatter([xs[0]], [ys[0]], marker="o", s=10)
        ax.scatter([xs[-1]], [ys[-1]], marker="x", s=14)

    ax.set_xlim(0, calib["world_width_m"])
    ax.set_ylim(0, calib["world_height_m"])
    ax.set_xlabel("x (m)")
    ax.set_ylabel("y (m)")
    ax.set_title("Agent trajectories (obs[0..2] denormalized)")
    ax.legend(fontsize=6, ncol=2)
    fig.tight_layout()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.out, dpi=120)
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
