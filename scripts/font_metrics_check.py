#!/usr/bin/env python3
"""Inspect a font and fail unless its printable ASCII metrics are monospaced."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from ttyinv.font_metrics import inspect_font


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("font", type=Path)
    parser.add_argument("--json", action="store_true", dest="json_output")
    parser.add_argument("--max-line-height", type=float, default=1.5)
    args = parser.parse_args()

    metrics = inspect_font(args.font)
    if args.json_output:
        print(json.dumps(metrics.as_dict(), indent=2, sort_keys=True))
    else:
        print(f"font: {args.font}")
        print(f"ascii monospace: {metrics.ascii_monospace}")
        print(f"fixed-pitch flag: {metrics.fixed_pitch_flag}")
        print(f"advance width: {metrics.advance_width}/{metrics.units_per_em} em")
        print(f"line height: {metrics.line_height_em:.4f} em")
        if metrics.missing_ascii:
            print("missing ASCII:", ", ".join(hex(value) for value in metrics.missing_ascii))

    if not metrics.ascii_monospace:
        print("font-metrics: printable ASCII glyphs do not share one advance width")
        return 1
    if metrics.line_height_em > args.max_line_height:
        print(f"font-metrics: line height {metrics.line_height_em:.4f} exceeds {args.max_line_height:.4f}")
        return 1
    print("font-metrics: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
