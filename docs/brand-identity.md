# Butter Paper brand identity

Butter Paper is the clear, lightweight professional tool for reviewing drawings and documents.

## Brand idea

**The transparent layer between a drawing and a decision.**

The name suggests tracing paper, overlays, iteration, and refinement. The brand should feel precise, approachable, independent, and calm. It must not imitate the dense or institutional character of larger document and construction suites.

## Position and promise

- **Position:** Professional PDF review without suite-shaped complexity.
- **Promise:** Clarity you can work on.
- **Audience:** People in architecture, engineering, and construction who need direct PDF review and markup tools.
- **Personality:** Technically clear, human, open, quietly confident.

## Voice

Use plain, direct language. Explain the task and outcome before the machinery behind it.

- Say **“Review drawings clearly”**, not **“Transform your document workflow.”**
- Say **“Choose your download”**, not **“Unlock the Butter Paper experience.”**
- Be capable without sounding institutional.
- Be friendly without becoming whimsical.

## Master palette

The machine-readable source is [`assets/brand/palette.json`](../assets/brand/palette.json).

| Role | Name | Value | Use |
| --- | --- | --- | --- |
| Primary | Butter | `#F7CD57` | Brand recognition, primary actions, stable identity; carries Ink text |
| Beta | Blueprint | `#2F6CB6` | Beta identity; carries white text |
| Human accent | Markup Coral | `#E7654C` | Review marks and editorial emphasis |
| Ink | Ink | `#1F1C17` | Headings, body text, dark surfaces, text on Butter |
| Ground | Vellum | `#F7F3EA` | Marketing backgrounds and warm open space |
| Primary tint | Cream | `#FAF0D6` | Butter-tinted surfaces and background washes |
| Quiet surface | Rule | `#E3D8BD` | Borders, panels, and subtle structure |
| Secondary ink | Pencil | `#6E685D` | Supporting text and technical labels |
| Link ink | Gold Ink | `#8A5A00` | Butter-family links and hover text on light grounds |

Butter leads the identity. It is too light for text on light grounds: put Ink text on Butter, and use Gold Ink where Butter-family colour must be read as text. Markup Coral represents the human act of review. Blueprint belongs to beta and should not compete with Butter elsewhere.

## Release channels

Each channel is the same folded butterfly in a different paper. They are not separate brands; the per-channel colours are in `channels` in the palette file.

- **Stable:** butter-yellow paper.
- **Beta:** blueprint-blue paper.

Stable and beta must use the same icon geometry. Use the canonical sources:

- [`butter-paper-origami.svg`](../assets/icon-source/butter-paper-origami.svg)
- [`butter-paper-origami-beta.svg`](../assets/icon-source/butter-paper-origami-beta.svg)

The butterfly is Akira Yoshizawa's traditional origami butterfly, folded from one sheet on a waterbomb base. Two triangular forewings sit under a nearly straight top edge, each notched where it meets the hindwings. The hindwings are one diamond flap folded down in front of the forewings and split up the middle, and a small tab is folded over the top centre. It is drawn about 16% taller than the folded model, close to square at about 1.13:1, so it fills square icon slots. Light falls from the upper left: each forewing bows slightly and dips into the centre valley fold, the right half is the shaded side, the hindwing flap's folded edge catches the light and casts a tight shadow on the forewings, and fine paper grain covers every face. Both channels share every fold, light and grain; only the paper colour differs.

The renderer centres the glyph optically, a little below its box centre because the wide forewings make it top-heavy. On macOS and Linux it spans 80% of the canvas width, clear of the tile's rounded corners; Windows taskbar and file icons fill their slot, and Start tiles centre it in a padded square.

Do not redraw or simplify the icon for marketing layouts. After changing a master, regenerate every application icon with `python3 assets/icon-source/render_app_icons.py` (needs Inkscape and Pillow).

## Visual system

- Use warm Vellum space with Ink typography.
- Use folded paper planes with soft directional light, fine paper grain and slightly softened corners.
- Use colour and shade to show depth. Do not add internal outlines to the butterfly.
- Keep marketing layouts spacious and technically ordered.
- Use Geist where the product font is available. Use the platform sans-serif stack elsewhere.
- Keep documents and PDF pages neutral. Brand colour belongs to the product around the document, not on the document itself.

## Colour proportion

For general marketing layouts, start with approximately 50% Vellum, 25% Ink or white content, 15% Butter, 6% Markup Coral, and 4% Blueprint. This is a guide, not a required formula. Use Blueprint only when it has a clear role.
