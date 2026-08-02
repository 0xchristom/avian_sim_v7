#!/usr/bin/env python3
"""6.3: validate biology constants against literature ranges.

MUST read the same constants as `avian_core/src/calibration.rs` via
`calibration_export.json` — never hardcode a second copy. Any edit to a
calibration constant must be re-exported (cargo run -p avian_core --bin
calibration_export); the Rust `calibration_export_matches_committed_json` test
fails the release gate on drift.

Usage:
  python analysis/check_biology.py            # validate the committed export
  python analysis/check_biology.py --strict   # same checks, exit 1 on ANY fail
"""
from __future__ import annotations

import argparse
import sys

from telemetry_io import load_calibration

CHECKS: list[tuple[str, callable]] = []


def check(name: str) -> callable:
    def deco(fn: callable) -> callable:
        CHECKS.append((name, fn))
        return fn

    return deco


@check("mass curve ordering")
def _(c: dict) -> list[str]:
    errs = []
    if not c["hatchling_mass_g"] < c["fledgling_mass_g"] < c["adult_mass_g"]:
        errs.append("expected hatchling < fledgling < adult mass")
    if not 300.0 <= c["adult_mass_g"] <= 350.0:
        errs.append(f"adult mass {c['adult_mass_g']} outside literature 300-350 g")
    return errs


@check("basal metabolic rate")
def _(c: dict) -> list[str]:
    errs = []
    bmr = c["adult_bmr_watts"]
    if not 3.5 <= bmr <= 4.5:
        errs.append(f"BMR {bmr} outside literature 3.5-4.5 W")
    if not (c["sampled"]["bmr_at_adult"] == bmr):
        errs.append("bmr_for_mass(adult) != ADULT_BMR_WATTS")
    return errs


@check("locomotion speeds + flight cost")
def _(c: dict) -> list[str]:
    errs = []
    if not 0.5 <= c["walk_speed_ms"] <= 1.5:
        errs.append(f"walk speed {c['walk_speed_ms']} outside 0.5-1.5 m/s")
    if not 10.0 <= c["fly_speed_ms"] <= 20.0:
        errs.append(f"flight speed {c['fly_speed_ms']} outside 10-20 m/s")
    if not c["walk_speed_ms"] < c["fly_speed_ms"]:
        errs.append("walk must be slower than fly")
    if not 5.0 <= c["flight_mr_multiplier"] <= 9.0:
        errs.append(f"flight MR {c['flight_mr_multiplier']} outside 5-9x")
    if c["sampled"]["flight_mr_walking"] != 1.0:
        errs.append("walking must not pay flight cost")
    if c["sampled"]["flight_mr_flying"] != c["flight_mr_multiplier"]:
        errs.append("flight MR at fly speed must equal the multiplier")
    return errs


@check("daily energy requirement")
def _(c: dict) -> list[str]:
    if not 100.0 <= c["daily_energy_requirement_kj"] <= 200.0:
        return [f"daily requirement {c['daily_energy_requirement_kj']} outside 100-200 kJ"]
    return []


@check("vision refinement")
def _(c: dict) -> list[str]:
    errs = []
    if not 15.0 <= c["binocular_overlap_degrees"] <= 35.0:
        errs.append(f"binocular overlap {c['binocular_overlap_degrees']} outside 15-35 deg")
    if not c["binocular_overlap_degrees"] < c["vision_fov_degrees"]:
        errs.append("binocular overlap must be < FOV")
    return errs


@check("vitality curve")
def _(c: dict) -> list[str]:
    errs = []
    s = c["sampled"]
    if abs(s["vitality_at_0"] - 1.0) > 1e-9:
        errs.append("vitality must be 1.0 at birth")
    if abs(s["vitality_at_t_mid"] - 0.5) > 1e-9:
        errs.append("vitality must cross 0.5 at the median wild lifespan")
    if s["vitality_at_max"] >= 0.001:
        errs.append("vitality must be below 0.001 at max wild lifespan")
    if not (3.0 <= c["vitality_t_mid_years"] <= 5.0):
        errs.append(f"median lifespan {c['vitality_t_mid_years']} outside 3-5 yr band")
    return errs


@check("weather & feather sanity")
def _(c: dict) -> list[str]:
    errs = []
    if not c["rain_feather_decay_multiplier"] > 1.0:
        errs.append("rain must accelerate feather decay (>1x)")
    if not 0.0 < c["rain_visibility_factor"] < 1.0:
        errs.append("rain visibility factor must be in (0,1)")
    if not c["wind_flight_mr_multiplier"] > 1.0:
        errs.append("wind must raise flight metabolic rate (>1x)")
    if not c["heat_bmr_multiplier"] > 1.0:
        errs.append("heat must raise BMR (>1x)")
    return errs


@check("predator capability (sick fleer must die, healthy may escape)")
def _(c: dict) -> list[str]:
    errs = []
    # Sick pigeon flees at half flight speed (SICK_SPEED_MULTIPLIER * FLY_SPEED_MS);
    # predator must outrun that but NOT a healthy full-speed pigeon.
    sick_flee = c["sick_speed_multiplier"] * c["fly_speed_ms"]
    if not c["predator_speed_ms"] > sick_flee:
        errs.append("predator must outrun a sick fleeing pigeon")
    if not c["predator_speed_ms"] < c["fly_speed_ms"]:
        errs.append("predator must NOT outrun a healthy pigeon (else fleeing is pointless)")
    if not c["predator_detection_radius_m"] >= c["predator_contact_distance_m"]:
        errs.append("detection radius must be >= contact distance")
    return errs


@check("reward sign conventions")
def _(c: dict) -> list[str]:
    errs = []
    if not c["reward_grain"] > 0.0:
        errs.append("grain reward must be positive")
    if not c["reward_captured"] < 0.0:
        errs.append("capture reward must be negative")
    if not c["reward_flee_success"] > 0.0:
        errs.append("flee-success reward must be positive")
    return errs


def main() -> None:
    ap = argparse.ArgumentParser(description="Validate biology constants (6.3).")
    ap.add_argument("--strict", action="store_true", help="exit 1 on any failed check")
    args = ap.parse_args()

    c = load_calibration()
    all_errors: list[str] = []
    for name, fn in CHECKS:
        errors = fn(c)
        if errors:
            for e in errors:
                print(f"FAIL  [{name}] {e}")
                all_errors.append(e)
        else:
            print(f"PASS  [{name}]")

    if all_errors:
        print(f"\n{len(all_errors)} biology check(s) failed")
        sys.exit(1 if args.strict else 0)
    print(f"\nall {len(CHECKS)} biology checks passed")


if __name__ == "__main__":
    main()
