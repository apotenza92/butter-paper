#!/usr/bin/env python3
"""Frame a review-build window capture as a product screenshot.

Usage: compose.py CAPTURE OUTPUT HEADLINE [--dark]

CAPTURE is a 2x capture of a 1280 x 780 window (2560 x 1560 pixels). The
output is 2880 x 1800: the headline above the window, on Vellum (or on Ink
with --dark). The window keeps its own pixels, so its text stays sharp.
Needs Pillow and the Geist font (https://vercel.com/font).
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

CANVAS = (2880, 1800)
WINDOW = (2560, 1560)
WINDOW_TOP = 200
RADIUS = 24
VELLUM = (247, 243, 234)
INK = (31, 28, 23)
RULE = (227, 216, 189)

FONT_DIRS = [
    Path.home() / "Library/Fonts",
    Path("/Library/Fonts"),
    Path.home() / ".local/share/fonts",
    Path("/usr/share/fonts"),
    Path.home() / "AppData/Local/Microsoft/Windows/Fonts",
    Path("C:/Windows/Fonts"),
]


def geist(size: int) -> ImageFont.FreeTypeFont:
    for name in ("Geist-SemiBold.otf", "Geist-SemiBold.ttf"):
        for folder in FONT_DIRS:
            found = next(folder.rglob(name), None) if folder.is_dir() else None
            if found:
                return ImageFont.truetype(str(found), size)
    sys.exit("Geist SemiBold is not installed")


def rounded_mask(size: tuple[int, int], radius: int) -> Image.Image:
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size[0] - 1, size[1] - 1), radius, fill=255)
    return mask


def main() -> None:
    args = [arg for arg in sys.argv[1:] if not arg.startswith("--")]
    if len(args) != 3:
        sys.exit(__doc__)
    capture, output, headline = args
    dark = "--dark" in sys.argv
    ground, text, edge = (INK, VELLUM, (70, 64, 55)) if dark else (VELLUM, INK, RULE)

    window = Image.open(capture).convert("RGB")
    if window.size != WINDOW:
        sys.exit(f"{capture} is {window.size}, not {WINDOW}: capture a 1280 x 780 window at 2x")

    canvas = Image.new("RGB", CANVAS, ground)
    left = (CANVAS[0] - WINDOW[0]) // 2

    # A soft shadow under the window, as light from above casts it.
    shadow = Image.new("L", CANVAS, 0)
    ImageDraw.Draw(shadow).rounded_rectangle(
        (left, WINDOW_TOP + 18, left + WINDOW[0], WINDOW_TOP + WINDOW[1] + 18), RADIUS, fill=90 if not dark else 160
    )
    shadow = shadow.filter(ImageFilter.GaussianBlur(36))
    canvas.paste(Image.new("RGB", CANVAS, (0, 0, 0)), (0, 0), shadow)

    canvas.paste(window, (left, WINDOW_TOP), rounded_mask(WINDOW, RADIUS))
    ImageDraw.Draw(canvas).rounded_rectangle(
        (left, WINDOW_TOP, left + WINDOW[0] - 1, WINDOW_TOP + WINDOW[1] - 1), RADIUS, outline=edge, width=2
    )

    font = geist(76)
    draw = ImageDraw.Draw(canvas)
    width = draw.textlength(headline, font=font)
    ascent, descent = font.getmetrics()
    draw.text(((CANVAS[0] - width) / 2, (WINDOW_TOP - ascent - descent) / 2 + 4), headline, font=font, fill=text)

    canvas.save(output, optimize=True)


if __name__ == "__main__":
    main()
