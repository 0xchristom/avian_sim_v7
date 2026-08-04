"""Shared calibration loader for the 6.3 analysis scripts.

Loads `calibration_export.json` so every script reads the compiled constants
instead of a second hardcoded copy.
"""
from __future__ import annotations

import json
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
