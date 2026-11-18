#!/usr/bin/env python3
"""Build the fictional Harbour Lane Studio drawing set for screenshots.

Usage: make_set.py

Derives sheets A-102 and A-103 from harbour-lane-a101.svg by renaming its
rooms and title, exports each sheet to PDF with Inkscape, and joins the three
into harbour-lane-set.pdf with qpdf. Run it from any directory; it writes
beside itself. Needs Inkscape and qpdf.
"""

import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
BASE = HERE / "harbour-lane-a101.svg"

SHEETS = {
    "a102": {
        ">Ground Floor Plan<": ">First Floor Plan<",
        ">A-101<": ">A-102<",
        ">MEETING<": ">OPEN OFFICE<",
        ">STUDIO<": ">DESIGN STUDIO<",
        ">WORKSHOP<": ">LIBRARY<",
        ">KITCHEN<": ">BREAKOUT<",
        ">RECEPTION<": ">BOARDROOM<",
        ">ENTRY<": "><",
    },
    "a103": {
        ">Ground Floor Plan<": ">Second Floor Plan<",
        ">A-101<": ">A-103<",
        ">MEETING<": ">STUDIO A<",
        ">STUDIO<": ">STUDIO B<",
        ">WORKSHOP<": ">MODEL SHOP<",
        ">KITCHEN<": ">TERRACE<",
        ">RECEPTION<": ">GALLERY<",
        ">ENTRY<": "><",
    },
}


def export(svg: Path, pdf: Path) -> None:
    subprocess.run(
        ["inkscape", str(svg), "--export-type=pdf", f"--export-filename={pdf}"],
        check=True,
        capture_output=True,
    )


def main() -> None:
    base = BASE.read_text()
    pdfs = [HERE / "harbour-lane-a101.pdf"]
    for name, replacements in SHEETS.items():
        sheet = base
        for old, new in replacements.items():
            if old not in sheet:
                sys.exit(f"{old} is not in the A-101 drawing")
            sheet = sheet.replace(old, new)
        svg = HERE / f"harbour-lane-{name}.svg"
        svg.write_text(sheet)
        pdf = HERE / f"harbour-lane-{name}.pdf"
        export(svg, pdf)
        svg.unlink()
        pdfs.append(pdf)
    subprocess.run(
        ["qpdf", "--empty", "--pages", *map(str, pdfs), "--", str(HERE / "harbour-lane-set.pdf")],
        check=True,
    )
    for pdf in pdfs[1:]:
        pdf.unlink()


if __name__ == "__main__":
    main()
