#!/usr/bin/env python3
"""Create visual calibration artifacts from a reference image and a ttyinv render.

The reference image is deliberately supplied at runtime and is never copied into the
repository. Keep the output directory outside the source tree when the reference
contains private or third-party material.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image, ImageChops, ImageEnhance, ImageStat


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path, help="cropped reference page image")
    parser.add_argument("rendered", type=Path, help="rendered ttyinv page image")
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument(
        "--no-resize",
        action="store_true",
        help="fail instead of resizing the rendered image to the reference dimensions",
    )
    args = parser.parse_args()

    reference = Image.open(args.reference).convert("RGB")
    rendered = Image.open(args.rendered).convert("RGB")
    if rendered.size != reference.size:
        if args.no_resize:
            parser.error(
                f"image dimensions differ: reference={reference.size}, rendered={rendered.size}"
            )
        rendered = rendered.resize(reference.size, Image.Resampling.LANCZOS)

    args.out_dir.mkdir(parents=True, exist_ok=True)

    overlay = Image.blend(reference, rendered, 0.5)
    overlay.save(args.out_dir / "overlay.png")

    difference = ImageChops.difference(reference, rendered)
    ImageEnhance.Contrast(difference).enhance(3).save(args.out_dir / "difference.png")

    side_by_side = Image.new("RGB", (reference.width * 2, reference.height))
    side_by_side.paste(reference, (0, 0))
    side_by_side.paste(rendered, (reference.width, 0))
    side_by_side.save(args.out_dir / "side-by-side.png")

    mean = ImageStat.Stat(difference).mean
    mae = sum(mean) / len(mean)
    print(f"reference: {reference.width}x{reference.height}")
    print(f"mean absolute RGB error: {mae:.3f} / 255")
    print(f"wrote {args.out_dir}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
