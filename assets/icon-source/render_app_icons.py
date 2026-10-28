"""Renders the application icons from the origami masters:

- assets/butter-paper-icon*.png, the 1024 px artwork per channel;
- assets/app and assets/app/beta: icon.png, icon.icns, icon.ico, the Linux
  sizes, the macOS Icon Composer artwork and generated-icon-manifest.json;
- the Microsoft Store images, through render_store_assets.py.

Needs Inkscape on PATH and Pillow:

    python3 assets/icon-source/render_app_icons.py
"""

import hashlib
import io
import json
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

import render_store_assets

SOURCE = Path(__file__).resolve().parent
ASSETS = SOURCE.parent
ROOT = ASSETS.parent
CANVAS = 1024
# The glyph spans 80% of the canvas width, which keeps the wingtips clear of
# the macOS tile's rounded corners, and is optically centred.
VISIBLE_WIDTH = 0.80
LINUX_SIZES = (16, 22, 24, 32, 48, 64, 72, 96, 128, 256, 512, 1024)
ICNS_ENTRIES = (
    ("icp4", 16), ("icp5", 32), ("icp6", 64), ("ic07", 128), ("ic08", 256),
    ("ic09", 512), ("ic10", 1024), ("ic11", 32), ("ic12", 64), ("ic13", 256),
    ("ic14", 512),
)
CHANNELS = (
    {
        "svg": "butter-paper-origami.svg",
        "artwork": ("butter-paper-icon.png", "butter-paper-icon-dark.png"),
        "output": ASSETS / "app",
        "icon_composer": "macos/Butter Paper.icon/Assets",
    },
    {
        "svg": "butter-paper-origami-beta.svg",
        "artwork": ("butter-paper-icon-beta.png", "butter-paper-icon-beta-dark.png"),
        "output": ASSETS / "app" / "beta",
        "icon_composer": "macos/Butter Paper Beta.icon/Assets",
    },
)


def glyph(svg, width):
    """The SVG's drawing, cropped to its visible pixels, `width` px wide."""
    with tempfile.TemporaryDirectory() as work:
        png = Path(work) / "glyph.png"
        subprocess.run(
            ["inkscape", str(svg), "--export-type=png", f"--export-filename={png}",
             "--export-area-drawing", "-w", str(width * 4)],
            check=True, capture_output=True,
        )
        image = Image.open(png).convert("RGBA")
    image = image.crop(image.getchannel("A").getbbox())
    height = round(width * image.height / image.width)
    return image.resize((width, height), Image.LANCZOS)


def artwork(svg):
    art = glyph(svg, round(CANVAS * VISIBLE_WIDTH))
    canvas = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    top = (CANVAS - art.height) // 2 + round(render_store_assets.optical_drop(art))
    canvas.alpha_composite(art, ((CANVAS - art.width) // 2, top))
    return canvas


def png_bytes(image):
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    return buffer.getvalue()


def icns(artwork_image):
    chunks = b""
    for kind, size in ICNS_ENTRIES:
        data = png_bytes(artwork_image.resize((size, size), Image.LANCZOS))
        chunks += kind.encode("ascii") + struct.pack(">I", len(data) + 8) + data
    return b"icns" + struct.pack(">I", len(chunks) + 8) + chunks


def ico(artwork_image):
    """Windows draws icons unplated, so the glyph fills each image."""
    glyph_only = artwork_image.crop(artwork_image.getchannel("A").getbbox())
    frames = [render_store_assets.render(glyph_only, (size, size))
              for size in render_store_assets.ICO_SIZES]
    buffer = io.BytesIO()
    frames[-1].save(buffer, format="ICO",
                    sizes=[(size, size) for size in render_store_assets.ICO_SIZES],
                    append_images=frames[:-1])
    return buffer.getvalue()


def render_channel(channel):
    image = artwork(SOURCE / channel["svg"])
    artwork_bytes = png_bytes(image)
    for name in channel["artwork"]:
        (ASSETS / name).write_bytes(artwork_bytes)
    files = {
        "icon.png": png_bytes(image.resize((512, 512), Image.LANCZOS)),
        "icon.icns": icns(image),
        "icon.ico": ico(image),
        f"{channel['icon_composer']}/01-artwork.png": artwork_bytes,
        f"{channel['icon_composer']}/01-artwork-dark.png": artwork_bytes,
    }
    for size in LINUX_SIZES:
        files[f"linux/{size}x{size}.png"] = png_bytes(image.resize((size, size), Image.LANCZOS))
    output = channel["output"]
    shutil.rmtree(output / "linux", ignore_errors=True)
    for relative, data in files.items():
        path = output / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    return files


def write_manifest(channel, files):
    source = ASSETS / channel["artwork"][0]
    manifest = {
        "schemaVersion": 1,
        "source": source.relative_to(ROOT).as_posix(),
        "sourceSha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "outputs": {
            relative: {
                "sha256": hashlib.sha256((channel["output"] / relative).read_bytes()).hexdigest(),
                "size": (channel["output"] / relative).stat().st_size,
            }
            for relative in sorted(files)
        },
    }
    (channel["output"] / "generated-icon-manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n"
    )


def main():
    rendered = [(channel, render_channel(channel)) for channel in CHANNELS]
    # The Store images and the stable icon.ico come from the stable Linux 1024.
    render_store_assets.main()
    for channel, files in rendered:
        write_manifest(channel, files)


if __name__ == "__main__":
    main()
