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

# Image names, sizes at scale 100, and the share of the tile's height the
# icon may fill. Icons fill their slot; Start tiles centre the icon in a
# padded square, as Windows' own tiles do.
TILES = {
    "Square44x44Logo": ((44, 44), 1.0),
    "Square150x150Logo": ((150, 150), 0.6),
    "Wide310x150Logo": ((310, 150), 0.6),
    "StoreLogo": ((50, 50), 1.0),
}
SCALES = (100, 125, 150, 200, 400)
# Taskbar, Start and File Explorer icons at every display scale Windows asks
# for. Windows draws the unplated forms as they are, choosing lightunplated
# under the light theme; without that form it falls back to the plated image on
# an accent-coloured square.
TARGET_SIZES = (16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 256)
ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 256)


def optical_drop(icon):
    """How far below box-centring `icon` sits to look centred, in its own
    pixels: a third of the distance from its box centre down to its visual
    centre, the alpha-weighted mean row. A top-heavy butterfly sits a little
    low."""
    rows = icon.getchannel("A").resize((1, icon.height), Image.BOX)
    weights = [rows.getpixel((0, y)) for y in range(icon.height)]
    centre = sum(y * w for y, w in enumerate(weights)) / sum(weights)
    return ((icon.height - 1) / 2 - centre) / 3


def render(icon, size, fill=1.0):
    """The icon as large as fits in a centred square `fill` of `size`'s
    height (or the whole of `size` at 1.0), optically centred."""
    width, height = size
    box = (width, height) if fill == 1.0 else (height * fill, height * fill)
    scale = min(box[0] / icon.width, box[1] / icon.height)
    fitted = (max(1, round(icon.width * scale)), max(1, round(icon.height * scale)))
    image = Image.new("RGBA", size, (0, 0, 0, 0))
    scaled = icon.resize(fitted, Image.LANCZOS)
    top = (height - fitted[1]) // 2 + round(optical_drop(icon) * scale)
    image.paste(scaled, ((width - fitted[0]) // 2, max(0, min(top, height - fitted[1]))), scaled)
    return image


def main():
    icon = Image.open(SOURCE).convert("RGBA")
    icon = icon.crop(icon.getchannel("A").getbbox())
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for stale in OUTPUT.glob("*.png"):
        stale.unlink()
    for name, ((width, height), fill) in TILES.items():
        for scale in SCALES:
            size = (width * scale // 100, height * scale // 100)
            render(icon, size, fill).save(OUTPUT / f"{name}.scale-{scale}.png")
    for target in TARGET_SIZES:
        image = render(icon, (target, target))
        image.save(OUTPUT / f"Square44x44Logo.targetsize-{target}.png")
        for form in ("unplated", "lightunplated"):
            image.save(OUTPUT / f"Square44x44Logo.targetsize-{target}_altform-{form}.png")
    frames = [render(icon, (size, size)) for size in ICO_SIZES]
    frames[-1].save(ICO, sizes=[(size, size) for size in ICO_SIZES], append_images=frames[:-1])


if __name__ == "__main__":
    main()
