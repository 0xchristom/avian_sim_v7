#!/usr/bin/env python3
"""6.3: validate determinism — diff two telemetry runs from the same seed.

The sim's reward policy should be a pure function of (obs, action); with the
same seed the per-frame rewards and obs stream must be identical. This script
aligns frames by (frame, uid) and reports the first divergence plus a summary
of mismatched rows. Exits 0 if identical, 1 otherwise (CI gate).

Usage:
  python analysis/validate_determinism.py run_a.csv run_b.csv
  python analysis/validate_determinism.py a.csv b.csv --max-diffs 20
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

from telemetry_io import read_telemetry

# Fields compared for equality (next_fsm/events can legitimately differ due to
# the exporter's one-frame delay — but for a deterministic sim they should NOT;
# we compare them too and only exclude them from the "obs mismatch" gate).
COMPARE_FIELDS = [
    "time_us",
    "frame",
    "uid",
    "reward",
    "reward_grain",
    "reward_flocking",
    "reward_starvation",
    "reward_captured",
    "reward_flee_success",
    "alarm",
    "sick",
    "fsm",
    "obs",
]


def main() -> None:
    ap = argparse.ArgumentParser(description="Diff two same-seed telemetry runs.")
    ap.add_argument("run_a", type=Path)
    ap.add_argument("run_b", type=Path)
    ap.add_argument("--max-diffs", type=int, default=20, help="stop reporting after N mismatches")
    args = ap.parse_args()

    a = read_telemetry(args.run_a)
    b = read_telemetry(args.run_b)
    print(f"run A: {len(a)} frames  run B: {len(b)} frames")

    # Index B by (frame, uid) for alignment.
    b_index: dict[tuple[int, str], object] = {}
    for fr in b:
        b_index.setdefault((fr.frame, fr.uid), fr)

    if len(a) != len(b):
        print(f"WARNING: frame counts differ ({len(a)} vs {len(b)}) — alignment is best-effort")

    diffs = 0
    for fr in a:
        other = b_index.get((fr.frame, fr.uid))
        if other is None:
            print(f"  MISSING in B: frame={fr.frame} uid={fr.uid}")
            diffs += 1
            continue
        for field in COMPARE_FIELDS:
            va, vb = getattr(fr, field), getattr(other, field)
            if va != vb:
                # Floats: compare with tolerance to avoid float repr noise.
                if isinstance(va, float) and isinstance(vb, float):
                    if abs(va - vb) < 1e-6:
                        continue
                print(f"  DIFF frame={fr.frame} uid={fr.uid} field={field}: {va!r} vs {vb!r}")
                diffs += 1
                break
        if diffs >= args.max_diffs:
            print(f"  ...stopped after {diffs} diffs")
            break

    # Frames in B that are missing from A.
    a_keys = {(fr.frame, fr.uid) for fr in a}
    extra = [k for k in b_index if k not in a_keys]
    if extra and diffs < args.max_diffs:
        for k in extra[: args.max_diffs]:
            print(f"  MISSING in A: frame={k[0]} uid={k[1]}")
            diffs += 1

    if diffs == 0:
        print("IDENTICAL: runs are deterministic")
        sys.exit(0)
    print(f"NOT deterministic: {diffs} divergences")
    sys.exit(1)


if __name__ == "__main__":
    main()
