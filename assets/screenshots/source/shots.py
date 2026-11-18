#!/usr/bin/env python3
"""Write the review-driver scripts that stage the product screenshots.

Usage: shots.py OUTPUT_DIR

Each script stages one shot on the fictional Harbour Lane Studio set
(harbour-lane-set.pdf) in a 1280 x 780 window and saves it with `shot`.
Positions on the drawing are given in millimetres from its top left, as the
SVG draws it, and converted to PDF points from the page's bottom left, which
the driver's page commands take. Frame the captures with compose.py.
"""

import sys
from pathlib import Path

PAGE_HEIGHT_MM = 297.0
POINTS_PER_MM = 72.0 / 25.4


def pt(x_mm: float, y_mm: float) -> str:
    """A drawing point in millimetres as PDF points on page 1."""
    return f"{x_mm * POINTS_PER_MM:.1f} {(PAGE_HEIGHT_MM - y_mm) * POINTS_PER_MM:.1f}"


def start(appearance: str = "light") -> list[str]:
    return [
        "place 0 -9000 9000 1280 780",
        f"appearance {appearance}",
        "wait 3500",
        "move 0 640 900",
        "wait 400",
    ]


def click(x: float, y: float, page: int = 1) -> list[str]:
    return [f"page-click 0 {page} {pt(x, y)}", "wait 300"]


def box(tool_key: str, a: tuple[float, float], b: tuple[float, float]) -> list[str]:
    """Draw with a two-click tool: one corner, then the other."""
    return [f"key 0 {tool_key}", "wait 300", *click(*a), f"page-move 0 1 {pt(*b)}", "wait 300", *click(*b)]


def markups() -> list[str]:
    return [
        # A captioned cloud around the kitchen bench.
        *box("k", (240, 83), (195, 45)),
        "wait 1200",
        "type 0 Confirm bench length with joiner",
        "wait 500",
        "key 0 enter",
        "wait 600",
        # A cloud at the entry.
        *box("c", (247, 171), (269, 194)),
        "wait 500",
        # The reception label highlighted.
        "key 0 h",
        "wait 300",
        f"page-drag 0 1 {pt(231, 130.8)} {pt(256, 130.8)}",
        "wait 600",
        # A box around the meeting table.
        *box("r", (70, 63), (106, 97)),
        "wait 500",
        # A note beside the studio desks.
        *box("t", (52, 176), (126, 183)),
        "wait 1200",
        "type 0 Relocate desks to suit new layout",
        "wait 500",
        "key 0 escape",
        "wait 800",
        "key 0 escape",
        "wait 300",
        "key 0 v",
        "wait 300",
    ]


def hero(appearance: str) -> list[str]:
    return [
        *start(appearance),
        *markups(),
        # The entry's cloud selected, its properties open.
        *click(247, 182),
        "wait 600",
        "click-id 0 document-workspace-rail-actions",
        "wait 900",
        "move 0 640 900",
        "wait 1200",
        "shot hero",
        "quit",
    ]


def thumbnails() -> list[str]:
    return [
        *start(),
        "click-id 0 document-left-rail-pages",
        "wait 1500",
        "move 0 640 900",
        "wait 1500",
        "shot pages",
        "quit",
    ]


def templates() -> list[str]:
    return [
        *start(),
        "click-id 0 document-tab-template-picker",
        "wait 1500",
        # Dot Grid, whose preview shows its grid.
        "click-id 0 template-picker-item-built-in-dots",
        "wait 800",
        "move 0 640 900",
        "wait 600",
        "shot templates",
        "quit",
    ]


def measure() -> list[str]:
    return [
        *start(),
        # The page's stated 1:100, from its title block.
        "page-scale",
        "wait 1000",
        "click-id 0 page-scale-apply",
        "wait 800",
        # The reception's area.
        "key 0 shift-alt-a",
        "wait 300",
        *click(199, 114),
        *click(291, 114),
        *click(291, 186),
        *click(199, 186),
        "key 0 enter",
        "wait 600",
        # The studio's width.
        "key 0 shift-alt-q",
        "wait 300",
        *click(41, 150),
        *click(123, 150),
        "key 0 enter",
        "wait 600",
        # The workshop's width as a dimension.
        *box("shift-l", (126, 165), (196, 165)),
        "wait 800",
        "key 0 enter",
        "wait 600",
        "key 0 v",
        "wait 300",
        # The area selected, its properties open, the view closer in.
        *click(199, 150),
        "wait 600",
        "click-id 0 document-workspace-rail-actions",
        "wait 900",
        f"page-move 0 1 {pt(200, 150)}",
        "wait 300",
        "set-zoom 0 140",
        "wait 1500",
        "move 0 640 900",
        "wait 1200",
        "shot measure",
        "quit",
    ]


def signing() -> list[str]:
    return [
        *start(),
        "click-id 0 document-workspace-signature-tool",
        "wait 1500",
        "move 0 640 900",
        "wait 800",
        "shot signing",
        "quit",
    ]


SCRIPTS = {
    "measure": measure(),
    "signing": signing(),
    "hero": hero("light"),
    "hero-dark": hero("dark"),
    "pages": thumbnails(),
    "templates": templates(),
}


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    for name, lines in SCRIPTS.items():
        (out / f"{name}.txt").write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
