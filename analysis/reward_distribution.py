#!/usr/bin/env python3
"""6.3: reward distribution histograms from a telemetry export.

Plots the per-frame total reward and each component (grain / flocking /
starvation / captured / flee_success), plus a text table of stats. Requires
matplotlib for the plots — degrades to a stats table without it.

Usage:
  python analysis/reward_distribution.py telemetry.csv
  python analysis/reward_distribution.py telemetry.csv --out rewards.png
"""
from __future__ import annotations

import argparse
from pathlib import Path
from statistics import fmean, median

from telemetry_io import read_telemetry


def main() -> None:
    ap = argparse.ArgumentParser(description="Reward histograms from telemetry.")
    ap.add_argument("telemetry", type=Path, help="telemetry CSV/JSONL file")
    ap.add_argument("--out", type=Path, default=Path("reward_distribution.png"), help="output PNG")
    args = ap.parse_args()

    frames = read_telemetry(args.telemetry)
    if not frames:
        print("no frames found")
        return

    comps = {
        "grain": [f.reward_grain for f in frames],
        "flocking": [f.reward_flocking for f in frames],
        "starvation": [f.reward_starvation for f in frames],
        "captured": [f.reward_captured for f in frames],
        "flee_success": [f.reward_flee_success for f in frames],
    }
    total = [f.reward for f in frames]

    def stats(name: str, vals: list[float]) -> None:
        print(
            f"{name:>12s}  n={len(vals):>6d}  mean={fmean(vals):+9.4f}  "
            f"median={median(vals):+9.4f}  min={min(vals):+9.4f}  max={max(vals):+9.4f}"
        )

    print("reward components (per-frame):")
    stats("total", total)
    for name, vals in comps.items():
        stats(name, vals)

    # Cumulative share of each component across the whole run (3.7 aggregates).
    sums = {name: sum(v) for name, v in comps.items()}
    total_sum = sum(sums.values())
    print("\ncumulative reward contribution:")
    if total_sum != 0:
        for name, s in sorted(sums.items(), key=lambda kv: -abs(kv[1])):
            print(f"  {name:>12s}  {s:+12.4f}  ({100 * s / total_sum:+7.2f}%)")

    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        print("\nmatplotlib not installed — stats table only; run: pip install matplotlib")
        return

    fig, axes = plt.subplots(2, 3, figsize=(12, 7))
    for ax, (name, vals) in zip(axes.flat, comps.items()):
        ax.hist(vals, bins=50, color="#4c72b0", alpha=0.8)
        ax.set_title(name)
        ax.set_ylabel("frames")
    axes.flat[-1].hist(total, bins=50, color="#dd8452", alpha=0.8)
    axes.flat[-1].set_title("total reward")
    axes.flat[-1].set_ylabel("frames")
    fig.tight_layout()
    args.out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(args.out, dpi=120)
    print(f"\nwrote {args.out}")


if __name__ == "__main__":
    main()
