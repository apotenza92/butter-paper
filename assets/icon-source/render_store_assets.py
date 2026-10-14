"""Renders the Microsoft Store package images in assets/app/windows-store
from the generated 1024 px app icon. Needs Pillow:

    python3 assets/icon-source/render_store_assets.py
"""

from pathlib import Path

from PIL import Image

ASSETS = Path(__file__).resolve().parent.parent
SOURCE = ASSETS / "app" / "linux" / "1024x1024.png"
OUTPUT = ASSETS / "app" / "windows-store"

# Image, size at scale 100, and the share of the height the icon fills.
TILES = {
    "Square44x44Logo": ((44, 44), 1.0),
    "Square150x150Logo": ((150, 150), 0.66),
    "Wide310x150Logo": ((310, 150), 0.66),
    "StoreLogo": ((50, 50), 1.0),
}
SCALES = (100, 200, 400)
# Taskbar, Start and File Explorer icons, which Windows draws unplated.
TARGET_SIZES = (16, 24, 32, 48, 256)


def render(icon, size, fill):
    width, height = size
    side = round(height * fill)
    image = Image.new("RGBA", size, (0, 0, 0, 0))
    scaled = icon.resize((side, side), Image.LANCZOS)
    image.paste(scaled, ((width - side) // 2, (height - side) // 2), scaled)
    return image


def main():
    icon = Image.open(SOURCE).convert("RGBA")
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for stale in OUTPUT.glob("*.png"):
        stale.unlink()
    for name, ((width, height), fill) in TILES.items():
        for scale in SCALES:
            size = (width * scale // 100, height * scale // 100)
            render(icon, size, fill).save(OUTPUT / f"{name}.scale-{scale}.png")
    for target in TARGET_SIZES:
        image = render(icon, (target, target), 1.0)
        image.save(OUTPUT / f"Square44x44Logo.targetsize-{target}.png")
        image.save(OUTPUT / f"Square44x44Logo.targetsize-{target}_altform-unplated.png")


if __name__ == "__main__":
    main()
