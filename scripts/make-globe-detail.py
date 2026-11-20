#!/usr/bin/env python3
"""Pack NASA's 21600 × 10800 equirectangular Blue Marble JPEG into regional JPEGs."""

import argparse
from io import BytesIO
from pathlib import Path
import struct

from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    Image.MAX_IMAGE_PIXELS = 21600 * 10800
    size, columns, rows = 900, 24, 12
    with Image.open(args.source) as image:
        if image.size != (size * columns, size * rows):
            parser.error("Expected NASA's native 21600 × 10800 global image")
        image = image.convert("RGB")
    # Left and top overlap let each patch sample smoothly at the dateline and
    # across adjoining patches. The exact same source pixels meet at a seam.
    parts = []
    offsets = [0]
    for row in range(rows):
        for column in range(columns):
            x, y = column * size, row * size
            left, top = max(0, x - 1), max(0, y - 1)
            part = Image.new("RGB", (size + 2, size + 2))
            part.paste(image.crop((left, top, min(image.width, x + size + 1), min(image.height, y + size + 1))),
                       (int(x == 0), int(y == 0)))
            if x == 0:
                part.paste(image.crop((image.width - 1, top, image.width, min(image.height, y + size + 1))),
                           (0, int(y == 0)))
            if column == columns - 1:
                part.paste(image.crop((0, top, 1, min(image.height, y + size + 1))),
                           (size + 1, int(y == 0)))
            if y == 0:
                part.paste(part.crop((0, 1, size + 2, 2)), (0, 0))
            if row == rows - 1:
                part.paste(part.crop((0, size, size + 2, size + 1)), (0, size + 1))
            encoded = BytesIO()
            part.save(encoded, "JPEG", quality=89, subsampling=0, optimize=True)
            data = encoded.getvalue()
            parts.append(data)
            offsets.append(offsets[-1] + len(data))
    with args.output.open("wb") as output:
        output.write(struct.pack(f"<{len(offsets)}I", *offsets))
        for part in parts:
            output.write(part)


if __name__ == "__main__":
    main()
