"""Renders the Windows icons: the Microsoft Store package images in
assets/app/windows-store and assets/app/icon.ico, which the executable embeds
for its windows and taskbar button. The source's transparent margin is cropped
off so the icon fills each image. render_app_icons.py runs it after
rendering assets/app/linux/1024x1024.png; on its own it needs Pillow:

    python3 assets/icon-source/render_store_assets.py
"""

from pathlib import Path

from PIL import Image

ASSETS = Path(__file__).resolve().parent.parent
SOURCE = ASSETS / "app" / "linux" / "1024x1024.png"
OUTPUT = ASSETS / "app" / "windows-store"
ICO = ASSETS / "app" / "icon.ico"

# Image names and sizes at scale 100.
TILES = {
    "Square44x44Logo": (44, 44),
    "Square150x150Logo": (150, 150),
    "Wide310x150Logo": (310, 150),
    "StoreLogo": (50, 50),
}
SCALES = (100, 125, 150, 200, 400)
# Taskbar, Start and File Explorer icons, which Windows draws unplated, at
# every display scale it asks for.
TARGET_SIZES = (16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256)
ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 256)


def render(icon, size):
    """The icon as large as fits in `size`, centred."""
    width, height = size
    scale = min(width / icon.width, height / icon.height)
    fitted = (max(1, round(icon.width * scale)), max(1, round(icon.height * scale)))
    image = Image.new("RGBA", size, (0, 0, 0, 0))
    scaled = icon.resize(fitted, Image.LANCZOS)
    image.paste(scaled, ((width - fitted[0]) // 2, (height - fitted[1]) // 2), scaled)
    return image


def main():
    icon = Image.open(SOURCE).convert("RGBA")
    icon = icon.crop(icon.getchannel("A").getbbox())
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for stale in OUTPUT.glob("*.png"):
        stale.unlink()
    for name, (width, height) in TILES.items():
        for scale in SCALES:
            size = (width * scale // 100, height * scale // 100)
            render(icon, size).save(OUTPUT / f"{name}.scale-{scale}.png")
    for target in TARGET_SIZES:
        image = render(icon, (target, target))
        image.save(OUTPUT / f"Square44x44Logo.targetsize-{target}.png")
        image.save(OUTPUT / f"Square44x44Logo.targetsize-{target}_altform-unplated.png")
    frames = [render(icon, (size, size)) for size in ICO_SIZES]
    frames[-1].save(ICO, sizes=[(size, size) for size in ICO_SIZES], append_images=frames[:-1])


if __name__ == "__main__":
    main()
