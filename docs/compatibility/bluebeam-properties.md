# Bluebeam Revu property audit

Generated from `scripts/bluebeam-compat/property-contract.mjs` schema v1.

> Evidence boundary: the installed Revu 21.2 manifest proves the application/capture environment. Properties without a one-variable PDF probe remain explicitly `unverified`; documentation is not treated as binary evidence.

## Scope and compatibility vocabulary

Audited: 20 tools (19 in Butter Paper plus Count). Excluded: select, pan, advanced-measurement, sketch, stamp, form.

Statuses: `exact`, `preserved-untouched`, `normalized`, `visual-only`, `unsupported`, `unverified`.

## Cross-tool primitive index

| Primitive | Used by |
| --- | --- |
| action | Text Box, Rectangle, Ellipse, Arc, Line, Arrow, Dimension, Length, Polylength, Area, Polyline, Polygon, Pen, Highlight, Cloud, Cloud+, Callout, Image, Snapshot, Count |
| color | Text Box, Rectangle, Ellipse, Arc, Line, Arrow, Dimension, Length, Polylength, Area, Polyline, Polygon, Pen, Highlight, Cloud, Cloud+, Callout, Count |
| count-symbol | Count |
| endpoint | Arc, Line, Arrow, Dimension, Length, Polylength, Polyline, Cloud+, Callout |
| endpoint-scale | Arc, Line, Arrow, Dimension, Length, Polylength, Polyline, Cloud+, Callout |
| finite-toggle | Text Box, Dimension, Length, Polylength, Area, Cloud+, Callout, Count |
| font-style | Text Box, Dimension, Length, Polylength, Area, Cloud+, Callout, Count |
| group | Count |
| hatch | Text Box, Rectangle, Ellipse, Area, Polygon, Cloud, Cloud+, Callout |
| line-style | Text Box, Rectangle, Ellipse, Arc, Line, Arrow, Dimension, Length, Polylength, Area, Polyline, Polygon, Pen, Cloud, Cloud+, Callout |
| numeric | Text Box, Rectangle, Ellipse, Arc, Line, Arrow, Dimension, Length, Polylength, Area, Polyline, Polygon, Pen, Highlight, Cloud, Cloud+, Callout, Image, Snapshot, Count |
| numeric-slider | Text Box, Rectangle, Ellipse, Arc, Line, Arrow, Dimension, Length, Polylength, Area, Polyline, Polygon, Pen, Highlight, Cloud, Cloud+, Callout, Image, Snapshot, Count |
| precision | Dimension, Length, Polylength, Area, Count |
| scale | Dimension, Length, Polylength, Area |
| searchable-font | Text Box, Dimension, Length, Polylength, Area, Cloud+, Callout, Count |
| select | Dimension, Length, Polylength, Area, Cloud+, Callout |
| switch | Text Box, Rectangle, Ellipse, Arc, Line, Arrow, Dimension, Length, Polylength, Area, Polyline, Polygon, Pen, Highlight, Cloud, Cloud+, Callout, Image, Snapshot, Count |
| text | Text Box, Rectangle, Ellipse, Arc, Line, Arrow, Dimension, Length, Polylength, Area, Polyline, Polygon, Pen, Highlight, Cloud, Cloud+, Callout, Image, Snapshot, Count |
| textarea | Text Box, Rectangle, Ellipse, Arc, Line, Arrow, Dimension, Length, Polylength, Area, Polyline, Polygon, Pen, Highlight, Cloud, Cloud+, Callout, Image, Snapshot, Count |
| units | Dimension, Length, Polylength, Area, Count |

## Compatibility gaps

| Tool | Property | Revu → Butter | Butter → Revu | Evidence note |
| --- | --- | --- | --- | --- |
| Text Box | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Author | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Color | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Fill Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Hatch Pattern | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Hatch Color | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Hatch Scale | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Text Color | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Font | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Font Size | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Auto Size | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Line Spacing | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Margin | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Alignment | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Font Style | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | X | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Y | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Width | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Height | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Text Box | Set as Default | unverified | unsupported | Documentation only; probe required |
| Text Box | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Rectangle | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Author | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Color | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Fill Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Hatch Pattern | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Hatch Color | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Hatch Scale | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | X | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Y | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Width | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Height | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Rectangle | Set as Default | unverified | unsupported | Documentation only; probe required |
| Rectangle | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Ellipse | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Author | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Color | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Fill Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Hatch Pattern | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Hatch Color | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Hatch Scale | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | X | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Y | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Width | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Height | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Ellipse | Set as Default | unverified | unsupported | Documentation only; probe required |
| Ellipse | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Arc | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Author | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Color | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Start | preserved-untouched | unverified | Documentation only; probe required |
| Arc | End | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Fill | preserved-untouched | unverified | Documentation only; probe required |
| Arc | X | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Y | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Width | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Height | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Arc | Set as Default | unverified | unsupported | Documentation only; probe required |
| Arc | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Line | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Line | Author | preserved-untouched | unverified | Documentation only; probe required |
| Line | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Line | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Line | Color | preserved-untouched | unverified | Documentation only; probe required |
| Line | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Line | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Line | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Line | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Line | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Line | Start | preserved-untouched | unverified | Documentation only; probe required |
| Line | End | preserved-untouched | unverified | Documentation only; probe required |
| Line | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Line | Fill | preserved-untouched | unverified | Documentation only; probe required |
| Line | X | preserved-untouched | unverified | Documentation only; probe required |
| Line | Y | preserved-untouched | unverified | Documentation only; probe required |
| Line | Width | preserved-untouched | unverified | Documentation only; probe required |
| Line | Height | preserved-untouched | unverified | Documentation only; probe required |
| Line | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Line | Set as Default | unverified | unsupported | Documentation only; probe required |
| Line | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Arrow | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Author | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Color | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Start | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | End | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Fill | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | X | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Y | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Width | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Height | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Arrow | Set as Default | unverified | unsupported | Documentation only; probe required |
| Arrow | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Dimension | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Author | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Label | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Color | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Start | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | End | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Fill | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Text Color | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Font | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Font Size | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Auto Size | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Line Spacing | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Margin | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Alignment | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Font Style | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Units | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Precision | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Show Caption | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Caption Position | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | X | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Y | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Width | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Height | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Dimension | Set as Default | unverified | unsupported | Documentation only; probe required |
| Dimension | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Length | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Length | Author | preserved-untouched | unverified | Documentation only; probe required |
| Length | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Length | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Length | Label | preserved-untouched | unverified | Documentation only; probe required |
| Length | Color | preserved-untouched | unverified | Documentation only; probe required |
| Length | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Length | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Length | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Length | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Length | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Length | Start | preserved-untouched | unverified | Documentation only; probe required |
| Length | End | preserved-untouched | unverified | Documentation only; probe required |
| Length | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Length | Fill | preserved-untouched | unverified | Documentation only; probe required |
| Length | Text Color | preserved-untouched | unverified | Documentation only; probe required |
| Length | Font | preserved-untouched | unverified | Documentation only; probe required |
| Length | Font Size | preserved-untouched | unverified | Documentation only; probe required |
| Length | Auto Size | preserved-untouched | unverified | Documentation only; probe required |
| Length | Line Spacing | preserved-untouched | unverified | Documentation only; probe required |
| Length | Margin | preserved-untouched | unverified | Documentation only; probe required |
| Length | Alignment | preserved-untouched | unverified | Documentation only; probe required |
| Length | Font Style | preserved-untouched | unverified | Documentation only; probe required |
| Length | Units | preserved-untouched | unverified | Documentation only; probe required |
| Length | Precision | preserved-untouched | unverified | Documentation only; probe required |
| Length | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Length | Show Caption | preserved-untouched | unverified | Documentation only; probe required |
| Length | Caption Position | preserved-untouched | unverified | Documentation only; probe required |
| Length | Slope | preserved-untouched | unverified | Documentation only; probe required |
| Length | X | preserved-untouched | unverified | Documentation only; probe required |
| Length | Y | preserved-untouched | unverified | Documentation only; probe required |
| Length | Width | preserved-untouched | unverified | Documentation only; probe required |
| Length | Height | preserved-untouched | unverified | Documentation only; probe required |
| Length | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Length | Set as Default | unverified | unsupported | Documentation only; probe required |
| Length | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Polylength | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Author | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Label | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Color | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Start | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | End | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Fill | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Text Color | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Font | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Font Size | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Auto Size | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Line Spacing | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Margin | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Alignment | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Font Style | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Units | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Precision | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Show Caption | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Caption Position | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Slope | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | X | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Y | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Width | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Height | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Polylength | Set as Default | unverified | unsupported | Documentation only; probe required |
| Polylength | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Area | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Area | Author | preserved-untouched | unverified | Documentation only; probe required |
| Area | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Area | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Area | Label | preserved-untouched | unverified | Documentation only; probe required |
| Area | Color | preserved-untouched | unverified | Documentation only; probe required |
| Area | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Area | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Area | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Area | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Area | Fill Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Area | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Area | Hatch Pattern | preserved-untouched | unverified | Documentation only; probe required |
| Area | Hatch Color | preserved-untouched | unverified | Documentation only; probe required |
| Area | Hatch Scale | preserved-untouched | unverified | Documentation only; probe required |
| Area | Text Color | preserved-untouched | unverified | Documentation only; probe required |
| Area | Font | preserved-untouched | unverified | Documentation only; probe required |
| Area | Font Size | preserved-untouched | unverified | Documentation only; probe required |
| Area | Auto Size | preserved-untouched | unverified | Documentation only; probe required |
| Area | Line Spacing | preserved-untouched | unverified | Documentation only; probe required |
| Area | Margin | preserved-untouched | unverified | Documentation only; probe required |
| Area | Alignment | preserved-untouched | unverified | Documentation only; probe required |
| Area | Font Style | preserved-untouched | unverified | Documentation only; probe required |
| Area | Units | preserved-untouched | unverified | Documentation only; probe required |
| Area | Precision | preserved-untouched | unverified | Documentation only; probe required |
| Area | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Area | Show Caption | preserved-untouched | unverified | Documentation only; probe required |
| Area | Caption Position | preserved-untouched | unverified | Documentation only; probe required |
| Area | X | preserved-untouched | unverified | Documentation only; probe required |
| Area | Y | preserved-untouched | unverified | Documentation only; probe required |
| Area | Width | preserved-untouched | unverified | Documentation only; probe required |
| Area | Height | preserved-untouched | unverified | Documentation only; probe required |
| Area | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Area | Set as Default | unverified | unsupported | Documentation only; probe required |
| Area | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Polyline | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Author | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Color | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Start | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | End | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Fill | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | X | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Y | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Width | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Height | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Polyline | Set as Default | unverified | unsupported | Documentation only; probe required |
| Polyline | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Polygon | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Author | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Color | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Fill Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Hatch Pattern | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Hatch Color | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Hatch Scale | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | X | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Y | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Width | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Height | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Polygon | Set as Default | unverified | unsupported | Documentation only; probe required |
| Polygon | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Pen | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Author | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Color | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Pen | X | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Y | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Width | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Height | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Pen | Set as Default | unverified | unsupported | Documentation only; probe required |
| Pen | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Highlight | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Author | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Color | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | X | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Y | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Width | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Height | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Highlight | Set as Default | unverified | unsupported | Documentation only; probe required |
| Highlight | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Cloud | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Author | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Color | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Fill Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Hatch Pattern | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Hatch Color | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Hatch Scale | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Cloud Size | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Invert | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | X | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Y | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Width | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Height | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Cloud | Set as Default | unverified | unsupported | Documentation only; probe required |
| Cloud | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Cloud+ | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Author | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Color | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Fill Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Hatch Pattern | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Hatch Color | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Hatch Scale | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Start | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | End | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Fill | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Text Color | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Font | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Font Size | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Auto Size | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Line Spacing | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Margin | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Alignment | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Font Style | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Cloud Size | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Invert | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Shape | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | X | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Y | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Width | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Height | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Cloud+ | Set as Default | unverified | unsupported | Documentation only; probe required |
| Cloud+ | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Callout | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Author | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Color | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Line Width | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Line Style | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Fill Color | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Fill Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Highlight | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Hatch Pattern | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Hatch Color | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Hatch Scale | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Start | preserved-untouched | unverified | Documentation only; probe required |
| Callout | End | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Scale | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Fill | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Text Color | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Font | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Font Size | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Auto Size | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Line Spacing | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Margin | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Alignment | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Font Style | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Cloud Size | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Invert | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Shape | preserved-untouched | unverified | Documentation only; probe required |
| Callout | X | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Y | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Width | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Height | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Callout | Set as Default | unverified | unsupported | Documentation only; probe required |
| Callout | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Image | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Image | Author | preserved-untouched | unverified | Documentation only; probe required |
| Image | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Image | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Image | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Image | X | preserved-untouched | unverified | Documentation only; probe required |
| Image | Y | preserved-untouched | unverified | Documentation only; probe required |
| Image | Width | preserved-untouched | unverified | Documentation only; probe required |
| Image | Height | preserved-untouched | unverified | Documentation only; probe required |
| Image | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Image | Set as Default | unverified | unsupported | Documentation only; probe required |
| Image | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Snapshot | Subject | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | Author | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | Comments | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | Locked | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | Opacity | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | X | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | Y | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | Width | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | Height | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | Rotation | preserved-untouched | unverified | Documentation only; probe required |
| Snapshot | Set as Default | unverified | unsupported | Documentation only; probe required |
| Snapshot | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |
| Count | Subject | preserved-untouched | exact | Documentation only; probe required |
| Count | Author | preserved-untouched | exact | Documentation only; probe required |
| Count | Comments | preserved-untouched | exact | Documentation only; probe required |
| Count | Locked | preserved-untouched | exact | Documentation only; probe required |
| Count | Label | preserved-untouched | unverified | Documentation only; probe required |
| Count | Color | preserved-untouched | exact | Documentation only; probe required |
| Count | Opacity | preserved-untouched | exact | Documentation only; probe required |
| Count | Text Color | preserved-untouched | normalized | Documentation only; probe required |
| Count | Font | preserved-untouched | normalized | Documentation only; probe required |
| Count | Font Size | preserved-untouched | exact | Documentation only; probe required |
| Count | Auto Size | preserved-untouched | visual-only | Documentation only; probe required |
| Count | Line Spacing | preserved-untouched | visual-only | Documentation only; probe required |
| Count | Margin | preserved-untouched | normalized | Documentation only; probe required |
| Count | Alignment | preserved-untouched | normalized | Documentation only; probe required |
| Count | Font Style | preserved-untouched | normalized | Documentation only; probe required |
| Count | Units | preserved-untouched | exact | Documentation only; probe required |
| Count | Precision | preserved-untouched | normalized | Documentation only; probe required |
| Count | Show Caption | preserved-untouched | normalized | Documentation only; probe required |
| Count | Symbol | unsupported | unsupported | Documentation only; probe required |
| Count | Scale | unsupported | unsupported | Documentation only; probe required |
| Count | Width | unsupported | unsupported | Documentation only; probe required |
| Count | Height | unsupported | unsupported | Documentation only; probe required |
| Count | Depth | unsupported | unsupported | Documentation only; probe required |
| Count | Group | unsupported | unsupported | Documentation only; probe required |
| Count | Set as Default | unverified | unsupported | Documentation only; probe required |
| Count | Add to Tool Chest | unverified | unsupported | Documentation only; probe required |

## Text Box

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Text Box tool](https://support.bluebeam.com/user-manual/menus/tools/text-box-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Opacity | Transparency of the interior fill independently of the outline. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | fill | /ca; unverified candidates: /FillOpacity; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Pattern | Pattern used instead of a solid interior fill. | Revu hatch-pattern library | "none" | appearance.fill-color is-not "transparent" | fill | none identified; unverified candidates: /BSIHatchPattern; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Color | Color of hatch lines. | sRGB/custom | "#000000" | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchColor; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Scale | Scales hatch spacing and pattern size. | {"min":50,"max":200,"step":1,"unit":"%"} | 100 | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchScale; AS: required for portable visual fidelity; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Text Color | Color of rendered text. | sRGB/custom | "#000000" | always | text | /DA, /DS; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font | Typeface used for text. | installed fonts | "Helvetica" | always | text | /DA, /DR/Font; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Size | Text size in points. | {"min":1,"max":144,"step":1,"unit":"pt"} | 12 | always | text | /DA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Auto Size | Automatically sizes the text container or font to fit content. | [false,true] | false | always | text/container | /DA; unverified candidates: /BSIAutoSize; AS: typically required; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Spacing | Vertical spacing between text baselines. | {"min":0.5,"max":3,"step":0.1,"unit":"×"} | 1 | always | text | /DS; unverified candidates: /BSILineSpacing; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Margin | Inset between text and its container border. | {"min":0,"max":72,"step":0.5,"unit":"pt"} | 3 | always | text/container | /RD; unverified candidates: /BSIMargin; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Alignment | Horizontal and vertical alignment of text within its box. | ["left","center","right","top","middle","bottom"] | "left/top" | always | text | /Q; unverified candidates: /BSIVerticalAlignment; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Style | Bold, italic, underline, strikethrough, superscript, and subscript text emphasis. | ["bold","italic","underline","strikethrough","superscript","subscript"] | [] | always | text | /DA, /DS; unverified candidates: /BSIFontStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Rectangle

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Rectangle tool](https://support.bluebeam.com/user-manual/menus/tools/rectangle-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Opacity | Transparency of the interior fill independently of the outline. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | fill | /ca; unverified candidates: /FillOpacity; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Pattern | Pattern used instead of a solid interior fill. | Revu hatch-pattern library | "none" | appearance.fill-color is-not "transparent" | fill | none identified; unverified candidates: /BSIHatchPattern; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Color | Color of hatch lines. | sRGB/custom | "#000000" | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchColor; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Scale | Scales hatch spacing and pattern size. | {"min":50,"max":200,"step":1,"unit":"%"} | 100 | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchScale; AS: required for portable visual fidelity; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Ellipse

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Ellipse tool](https://support.bluebeam.com/user-manual/menus/tools/ellipse-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Opacity | Transparency of the interior fill independently of the outline. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | fill | /ca; unverified candidates: /FillOpacity; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Pattern | Pattern used instead of a solid interior fill. | Revu hatch-pattern library | "none" | appearance.fill-color is-not "transparent" | fill | none identified; unverified candidates: /BSIHatchPattern; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Color | Color of hatch lines. | sRGB/custom | "#000000" | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchColor; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Scale | Scales hatch spacing and pattern size. | {"min":50,"max":200,"step":1,"unit":"%"} | 100 | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchScale; AS: required for portable visual fidelity; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Arc

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Arc tool](https://support.bluebeam.com/user-manual/menus/tools/arc-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Start | Marker drawn at the start of a line or leader. | Revu endpoint library | "none" | always | start endpoint | /LE/0; unverified candidates: /BSIStart; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | End | Marker drawn at the end of a line or leader. | Revu endpoint library | "none" | always | end endpoint | /LE/1; unverified candidates: /BSIEnd; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Scale | Scales endpoint markers relative to line width; Auto follows Revu sizing. | {"min":0,"max":500,"step":1,"unit":"%","special":"auto"} | "auto" | appearance.start-endpoint\|appearance.end-endpoint any-is-not "none" | endpoints | none identified; unverified candidates: /BSILineEndScale; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill | Fills endpoint markers that support open/filled variants. | [false,true] | true | appearance.start-endpoint\|appearance.end-endpoint supports-fill true | endpoints | /LE; unverified candidates: /BSILineEndFill; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Line

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Line tool](https://support.bluebeam.com/user-manual/menus/tools/line-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Start | Marker drawn at the start of a line or leader. | Revu endpoint library | "none" | always | start endpoint | /LE/0; unverified candidates: /BSIStart; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | End | Marker drawn at the end of a line or leader. | Revu endpoint library | "none" | always | end endpoint | /LE/1; unverified candidates: /BSIEnd; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Scale | Scales endpoint markers relative to line width; Auto follows Revu sizing. | {"min":0,"max":500,"step":1,"unit":"%","special":"auto"} | "auto" | appearance.start-endpoint\|appearance.end-endpoint any-is-not "none" | endpoints | none identified; unverified candidates: /BSILineEndScale; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill | Fills endpoint markers that support open/filled variants. | [false,true] | true | appearance.start-endpoint\|appearance.end-endpoint supports-fill true | endpoints | /LE; unverified candidates: /BSILineEndFill; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Arrow

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Line tool](https://support.bluebeam.com/user-manual/menus/tools/line-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Start | Marker drawn at the start of a line or leader. | Revu endpoint library | "none" | always | start endpoint | /LE/0; unverified candidates: /BSIStart; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | End | Marker drawn at the end of a line or leader. | Revu endpoint library | "none" | always | end endpoint | /LE/1; unverified candidates: /BSIEnd; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Scale | Scales endpoint markers relative to line width; Auto follows Revu sizing. | {"min":0,"max":500,"step":1,"unit":"%","special":"auto"} | "auto" | appearance.start-endpoint\|appearance.end-endpoint any-is-not "none" | endpoints | none identified; unverified candidates: /BSILineEndScale; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill | Fills endpoint markers that support open/filled variants. | [false,true] | true | appearance.start-endpoint\|appearance.end-endpoint supports-fill true | endpoints | /LE; unverified candidates: /BSILineEndFill; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Dimension

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Dimension tool](https://support.bluebeam.com/user-manual/menus/tools/dimension-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Label | Display label used by the Markups List and summaries. | text | "" | always | metadata | /Contents, /Subj; unverified candidates: /BSIColumnData; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Start | Marker drawn at the start of a line or leader. | Revu endpoint library | "none" | always | start endpoint | /LE/0; unverified candidates: /BSIStart; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | End | Marker drawn at the end of a line or leader. | Revu endpoint library | "none" | always | end endpoint | /LE/1; unverified candidates: /BSIEnd; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Scale | Scales endpoint markers relative to line width; Auto follows Revu sizing. | {"min":0,"max":500,"step":1,"unit":"%","special":"auto"} | "auto" | appearance.start-endpoint\|appearance.end-endpoint any-is-not "none" | endpoints | none identified; unverified candidates: /BSILineEndScale; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill | Fills endpoint markers that support open/filled variants. | [false,true] | true | appearance.start-endpoint\|appearance.end-endpoint supports-fill true | endpoints | /LE; unverified candidates: /BSILineEndFill; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Text Color | Color of rendered text. | sRGB/custom | "#000000" | always | text | /DA, /DS; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font | Typeface used for text. | installed fonts | "Helvetica" | always | text | /DA, /DR/Font; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Size | Text size in points. | {"min":1,"max":144,"step":1,"unit":"pt"} | 12 | always | text | /DA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Auto Size | Automatically sizes the text container or font to fit content. | [false,true] | false | always | text/container | /DA; unverified candidates: /BSIAutoSize; AS: typically required; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Spacing | Vertical spacing between text baselines. | {"min":0.5,"max":3,"step":0.1,"unit":"×"} | 1 | always | text | /DS; unverified candidates: /BSILineSpacing; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Margin | Inset between text and its container border. | {"min":0,"max":72,"step":0.5,"unit":"pt"} | 3 | always | text/container | /RD; unverified candidates: /BSIMargin; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Alignment | Horizontal and vertical alignment of text within its box. | ["left","center","right","top","middle","bottom"] | "left/top" | always | text | /Q; unverified candidates: /BSIVerticalAlignment; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Style | Bold, italic, underline, strikethrough, superscript, and subscript text emphasis. | ["bold","italic","underline","strikethrough","superscript","subscript"] | [] | always | text | /DA, /DS; unverified candidates: /BSIFontStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Units | Display unit for measured values. | ["page-scale","mm","cm","m","in","ft"] | "page-scale" | always | measurement caption | /Measure/U; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Precision | Number and fractional precision used for the displayed result. | unit-dependent decimal or fractional precision | "document scale" | always | measurement caption | /Measure/D; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Scale | Ratio converting page-space distance into real-world distance. | page scale or independent scale | "page scale" | always | measurement value | /Measure/R, /Measure/X; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Show Caption | Shows the measurement value/caption on the markup. | [false,true] | true | always | measurement caption | /Contents; unverified candidates: /BSICaption; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Caption Position | Places the measurement caption along or around the geometry. | ["inline","top","bottom"] | "inline" | measurement.caption is true | measurement caption | none identified; unverified candidates: /BSICaptionPosition; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Length

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Length measurement](https://support.bluebeam.com/user-manual/menus/tools/length-measurement.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Label | Display label used by the Markups List and summaries. | text | "" | always | metadata | /Contents, /Subj; unverified candidates: /BSIColumnData; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Start | Marker drawn at the start of a line or leader. | Revu endpoint library | "none" | always | start endpoint | /LE/0; unverified candidates: /BSIStart; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | End | Marker drawn at the end of a line or leader. | Revu endpoint library | "none" | always | end endpoint | /LE/1; unverified candidates: /BSIEnd; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Scale | Scales endpoint markers relative to line width; Auto follows Revu sizing. | {"min":0,"max":500,"step":1,"unit":"%","special":"auto"} | "auto" | appearance.start-endpoint\|appearance.end-endpoint any-is-not "none" | endpoints | none identified; unverified candidates: /BSILineEndScale; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill | Fills endpoint markers that support open/filled variants. | [false,true] | true | appearance.start-endpoint\|appearance.end-endpoint supports-fill true | endpoints | /LE; unverified candidates: /BSILineEndFill; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Text Color | Color of rendered text. | sRGB/custom | "#000000" | always | text | /DA, /DS; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font | Typeface used for text. | installed fonts | "Helvetica" | always | text | /DA, /DR/Font; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Size | Text size in points. | {"min":1,"max":144,"step":1,"unit":"pt"} | 12 | always | text | /DA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Auto Size | Automatically sizes the text container or font to fit content. | [false,true] | false | always | text/container | /DA; unverified candidates: /BSIAutoSize; AS: typically required; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Spacing | Vertical spacing between text baselines. | {"min":0.5,"max":3,"step":0.1,"unit":"×"} | 1 | always | text | /DS; unverified candidates: /BSILineSpacing; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Margin | Inset between text and its container border. | {"min":0,"max":72,"step":0.5,"unit":"pt"} | 3 | always | text/container | /RD; unverified candidates: /BSIMargin; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Alignment | Horizontal and vertical alignment of text within its box. | ["left","center","right","top","middle","bottom"] | "left/top" | always | text | /Q; unverified candidates: /BSIVerticalAlignment; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Style | Bold, italic, underline, strikethrough, superscript, and subscript text emphasis. | ["bold","italic","underline","strikethrough","superscript","subscript"] | [] | always | text | /DA, /DS; unverified candidates: /BSIFontStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Units | Display unit for measured values. | ["page-scale","mm","cm","m","in","ft"] | "page-scale" | always | measurement caption | /Measure/U; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Precision | Number and fractional precision used for the displayed result. | unit-dependent decimal or fractional precision | "document scale" | always | measurement caption | /Measure/D; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Scale | Ratio converting page-space distance into real-world distance. | page scale or independent scale | "page scale" | always | measurement value | /Measure/R, /Measure/X; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Show Caption | Shows the measurement value/caption on the markup. | [false,true] | true | always | measurement caption | /Contents; unverified candidates: /BSICaption; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Caption Position | Places the measurement caption along or around the geometry. | ["inline","top","bottom"] | "inline" | measurement.caption is true | measurement caption | none identified; unverified candidates: /BSICaptionPosition; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Slope | Applies rise/run or pitch adjustment to a length. | {"min":0,"max":null,"step":0.01,"unit":"ratio"} | 0 | always | calculated value | /Measure; unverified candidates: /BSISlope; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Polylength

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Polylength measurement](https://support.bluebeam.com/user-manual/menus/tools/polylength-measurement.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Label | Display label used by the Markups List and summaries. | text | "" | always | metadata | /Contents, /Subj; unverified candidates: /BSIColumnData; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Start | Marker drawn at the start of a line or leader. | Revu endpoint library | "none" | always | start endpoint | /LE/0; unverified candidates: /BSIStart; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | End | Marker drawn at the end of a line or leader. | Revu endpoint library | "none" | always | end endpoint | /LE/1; unverified candidates: /BSIEnd; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Scale | Scales endpoint markers relative to line width; Auto follows Revu sizing. | {"min":0,"max":500,"step":1,"unit":"%","special":"auto"} | "auto" | appearance.start-endpoint\|appearance.end-endpoint any-is-not "none" | endpoints | none identified; unverified candidates: /BSILineEndScale; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill | Fills endpoint markers that support open/filled variants. | [false,true] | true | appearance.start-endpoint\|appearance.end-endpoint supports-fill true | endpoints | /LE; unverified candidates: /BSILineEndFill; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Text Color | Color of rendered text. | sRGB/custom | "#000000" | always | text | /DA, /DS; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font | Typeface used for text. | installed fonts | "Helvetica" | always | text | /DA, /DR/Font; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Size | Text size in points. | {"min":1,"max":144,"step":1,"unit":"pt"} | 12 | always | text | /DA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Auto Size | Automatically sizes the text container or font to fit content. | [false,true] | false | always | text/container | /DA; unverified candidates: /BSIAutoSize; AS: typically required; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Spacing | Vertical spacing between text baselines. | {"min":0.5,"max":3,"step":0.1,"unit":"×"} | 1 | always | text | /DS; unverified candidates: /BSILineSpacing; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Margin | Inset between text and its container border. | {"min":0,"max":72,"step":0.5,"unit":"pt"} | 3 | always | text/container | /RD; unverified candidates: /BSIMargin; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Alignment | Horizontal and vertical alignment of text within its box. | ["left","center","right","top","middle","bottom"] | "left/top" | always | text | /Q; unverified candidates: /BSIVerticalAlignment; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Style | Bold, italic, underline, strikethrough, superscript, and subscript text emphasis. | ["bold","italic","underline","strikethrough","superscript","subscript"] | [] | always | text | /DA, /DS; unverified candidates: /BSIFontStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Units | Display unit for measured values. | ["page-scale","mm","cm","m","in","ft"] | "page-scale" | always | measurement caption | /Measure/U; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Precision | Number and fractional precision used for the displayed result. | unit-dependent decimal or fractional precision | "document scale" | always | measurement caption | /Measure/D; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Scale | Ratio converting page-space distance into real-world distance. | page scale or independent scale | "page scale" | always | measurement value | /Measure/R, /Measure/X; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Show Caption | Shows the measurement value/caption on the markup. | [false,true] | true | always | measurement caption | /Contents; unverified candidates: /BSICaption; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Caption Position | Places the measurement caption along or around the geometry. | ["inline","top","bottom"] | "inline" | measurement.caption is true | measurement caption | none identified; unverified candidates: /BSICaptionPosition; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Slope | Applies rise/run or pitch adjustment to a length. | {"min":0,"max":null,"step":0.01,"unit":"ratio"} | 0 | always | calculated value | /Measure; unverified candidates: /BSISlope; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Area

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Area measurement](https://support.bluebeam.com/user-manual/menus/tools/area-measurement.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Label | Display label used by the Markups List and summaries. | text | "" | always | metadata | /Contents, /Subj; unverified candidates: /BSIColumnData; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Opacity | Transparency of the interior fill independently of the outline. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | fill | /ca; unverified candidates: /FillOpacity; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Pattern | Pattern used instead of a solid interior fill. | Revu hatch-pattern library | "none" | appearance.fill-color is-not "transparent" | fill | none identified; unverified candidates: /BSIHatchPattern; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Color | Color of hatch lines. | sRGB/custom | "#000000" | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchColor; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Scale | Scales hatch spacing and pattern size. | {"min":50,"max":200,"step":1,"unit":"%"} | 100 | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchScale; AS: required for portable visual fidelity; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Text Color | Color of rendered text. | sRGB/custom | "#000000" | always | text | /DA, /DS; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font | Typeface used for text. | installed fonts | "Helvetica" | always | text | /DA, /DR/Font; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Size | Text size in points. | {"min":1,"max":144,"step":1,"unit":"pt"} | 12 | always | text | /DA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Auto Size | Automatically sizes the text container or font to fit content. | [false,true] | false | always | text/container | /DA; unverified candidates: /BSIAutoSize; AS: typically required; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Spacing | Vertical spacing between text baselines. | {"min":0.5,"max":3,"step":0.1,"unit":"×"} | 1 | always | text | /DS; unverified candidates: /BSILineSpacing; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Margin | Inset between text and its container border. | {"min":0,"max":72,"step":0.5,"unit":"pt"} | 3 | always | text/container | /RD; unverified candidates: /BSIMargin; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Alignment | Horizontal and vertical alignment of text within its box. | ["left","center","right","top","middle","bottom"] | "left/top" | always | text | /Q; unverified candidates: /BSIVerticalAlignment; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Style | Bold, italic, underline, strikethrough, superscript, and subscript text emphasis. | ["bold","italic","underline","strikethrough","superscript","subscript"] | [] | always | text | /DA, /DS; unverified candidates: /BSIFontStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Units | Display unit for measured values. | ["page-scale","mm","cm","m","in","ft"] | "page-scale" | always | measurement caption | /Measure/U; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Precision | Number and fractional precision used for the displayed result. | unit-dependent decimal or fractional precision | "document scale" | always | measurement caption | /Measure/D; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Scale | Ratio converting page-space distance into real-world distance. | page scale or independent scale | "page scale" | always | measurement value | /Measure/R, /Measure/X; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Show Caption | Shows the measurement value/caption on the markup. | [false,true] | true | always | measurement caption | /Contents; unverified candidates: /BSICaption; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Measurement | Caption Position | Places the measurement caption along or around the geometry. | ["inline","top","bottom"] | "inline" | measurement.caption is true | measurement caption | none identified; unverified candidates: /BSICaptionPosition; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Polyline

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Polyline tool](https://support.bluebeam.com/user-manual/menus/tools/polyline-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Start | Marker drawn at the start of a line or leader. | Revu endpoint library | "none" | always | start endpoint | /LE/0; unverified candidates: /BSIStart; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | End | Marker drawn at the end of a line or leader. | Revu endpoint library | "none" | always | end endpoint | /LE/1; unverified candidates: /BSIEnd; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Scale | Scales endpoint markers relative to line width; Auto follows Revu sizing. | {"min":0,"max":500,"step":1,"unit":"%","special":"auto"} | "auto" | appearance.start-endpoint\|appearance.end-endpoint any-is-not "none" | endpoints | none identified; unverified candidates: /BSILineEndScale; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill | Fills endpoint markers that support open/filled variants. | [false,true] | true | appearance.start-endpoint\|appearance.end-endpoint supports-fill true | endpoints | /LE; unverified candidates: /BSILineEndFill; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Polygon

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Polygon tool](https://support.bluebeam.com/user-manual/menus/tools/polygon-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Opacity | Transparency of the interior fill independently of the outline. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | fill | /ca; unverified candidates: /FillOpacity; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Pattern | Pattern used instead of a solid interior fill. | Revu hatch-pattern library | "none" | appearance.fill-color is-not "transparent" | fill | none identified; unverified candidates: /BSIHatchPattern; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Color | Color of hatch lines. | sRGB/custom | "#000000" | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchColor; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Scale | Scales hatch spacing and pattern size. | {"min":50,"max":200,"step":1,"unit":"%"} | 100 | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchScale; AS: required for portable visual fidelity; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Pen

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Pen tool](https://support.bluebeam.com/user-manual/menus/tools/pen-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Highlight

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Highlight tool](https://support.bluebeam.com/user-manual/menus/tools/highlight-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Cloud

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Cloud tool](https://support.bluebeam.com/user-manual/menus/tools/cloud-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Opacity | Transparency of the interior fill independently of the outline. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | fill | /ca; unverified candidates: /FillOpacity; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Pattern | Pattern used instead of a solid interior fill. | Revu hatch-pattern library | "none" | appearance.fill-color is-not "transparent" | fill | none identified; unverified candidates: /BSIHatchPattern; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Color | Color of hatch lines. | sRGB/custom | "#000000" | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchColor; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Scale | Scales hatch spacing and pattern size. | {"min":50,"max":200,"step":1,"unit":"%"} | 100 | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchScale; AS: required for portable visual fidelity; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Cloud Size | Controls the radius/frequency of cloud scallops. | {"min":0.5,"max":10,"step":0.5,"unit":"pt"} | 2 | always | cloud border | /BE/I; unverified candidates: /BSICloudSize; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Invert | Flips cloud scallops inward. | [false,true] | false | appearance.line-style is "cloud" | cloud border | none identified; unverified candidates: /BSICloudInverted; AS: required; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Cloud+

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Callout tool](https://support.bluebeam.com/user-manual/menus/tools/callout-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Opacity | Transparency of the interior fill independently of the outline. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | fill | /ca; unverified candidates: /FillOpacity; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Pattern | Pattern used instead of a solid interior fill. | Revu hatch-pattern library | "none" | appearance.fill-color is-not "transparent" | fill | none identified; unverified candidates: /BSIHatchPattern; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Color | Color of hatch lines. | sRGB/custom | "#000000" | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchColor; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Scale | Scales hatch spacing and pattern size. | {"min":50,"max":200,"step":1,"unit":"%"} | 100 | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchScale; AS: required for portable visual fidelity; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Start | Marker drawn at the start of a line or leader. | Revu endpoint library | "none" | always | start endpoint | /LE/0; unverified candidates: /BSIStart; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | End | Marker drawn at the end of a line or leader. | Revu endpoint library | "none" | always | end endpoint | /LE/1; unverified candidates: /BSIEnd; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Scale | Scales endpoint markers relative to line width; Auto follows Revu sizing. | {"min":0,"max":500,"step":1,"unit":"%","special":"auto"} | "auto" | appearance.start-endpoint\|appearance.end-endpoint any-is-not "none" | endpoints | none identified; unverified candidates: /BSILineEndScale; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill | Fills endpoint markers that support open/filled variants. | [false,true] | true | appearance.start-endpoint\|appearance.end-endpoint supports-fill true | endpoints | /LE; unverified candidates: /BSILineEndFill; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Text Color | Color of rendered text. | sRGB/custom | "#000000" | always | text | /DA, /DS; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font | Typeface used for text. | installed fonts | "Helvetica" | always | text | /DA, /DR/Font; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Size | Text size in points. | {"min":1,"max":144,"step":1,"unit":"pt"} | 12 | always | text | /DA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Auto Size | Automatically sizes the text container or font to fit content. | [false,true] | false | always | text/container | /DA; unverified candidates: /BSIAutoSize; AS: typically required; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Spacing | Vertical spacing between text baselines. | {"min":0.5,"max":3,"step":0.1,"unit":"×"} | 1 | always | text | /DS; unverified candidates: /BSILineSpacing; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Margin | Inset between text and its container border. | {"min":0,"max":72,"step":0.5,"unit":"pt"} | 3 | always | text/container | /RD; unverified candidates: /BSIMargin; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Alignment | Horizontal and vertical alignment of text within its box. | ["left","center","right","top","middle","bottom"] | "left/top" | always | text | /Q; unverified candidates: /BSIVerticalAlignment; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Style | Bold, italic, underline, strikethrough, superscript, and subscript text emphasis. | ["bold","italic","underline","strikethrough","superscript","subscript"] | [] | always | text | /DA, /DS; unverified candidates: /BSIFontStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Cloud Size | Controls the radius/frequency of cloud scallops. | {"min":0.5,"max":10,"step":0.5,"unit":"pt"} | 2 | always | cloud border | /BE/I; unverified candidates: /BSICloudSize; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Invert | Flips cloud scallops inward. | [false,true] | false | appearance.line-style is "cloud" | cloud border | none identified; unverified candidates: /BSICloudInverted; AS: required; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Shape | Chooses the text-box container geometry. | ["rectangle","circle","triangle","hexagon"] | "rectangle" | always | container | /Subtype, /RD; unverified candidates: /BSIShape; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Callout

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Callout tool](https://support.bluebeam.com/user-manual/menus/tools/callout-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Width | Thickness of the outline or path. | {"min":0,"max":20,"step":0.25,"unit":"pt"} | 1 | always | stroke | /BS/W, /Border/2; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Style | Solid, dashed, or named/custom pattern used for the stroke. | solid and Revu line-style library | "solid" | always | stroke | /BS/S, /BS/D; unverified candidates: /BSILineStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Color | Color inside a closed shape or text container. | sRGB/custom/transparent | "transparent" | always | fill | /IC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill Opacity | Transparency of the interior fill independently of the outline. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | fill | /ca; unverified candidates: /FillOpacity; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Highlight | Uses a highlight/blend treatment so underlying page content remains visible. | [false,true] | false | always | compositing | /BM; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Pattern | Pattern used instead of a solid interior fill. | Revu hatch-pattern library | "none" | appearance.fill-color is-not "transparent" | fill | none identified; unverified candidates: /BSIHatchPattern; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Color | Color of hatch lines. | sRGB/custom | "#000000" | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchColor; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Hatch Scale | Scales hatch spacing and pattern size. | {"min":50,"max":200,"step":1,"unit":"%"} | 100 | appearance.hatch-pattern is-not "none" | hatch | none identified; unverified candidates: /BSIHatchScale; AS: required for portable visual fidelity; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Start | Marker drawn at the start of a line or leader. | Revu endpoint library | "none" | always | start endpoint | /LE/0; unverified candidates: /BSIStart; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | End | Marker drawn at the end of a line or leader. | Revu endpoint library | "none" | always | end endpoint | /LE/1; unverified candidates: /BSIEnd; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Scale | Scales endpoint markers relative to line width; Auto follows Revu sizing. | {"min":0,"max":500,"step":1,"unit":"%","special":"auto"} | "auto" | appearance.start-endpoint\|appearance.end-endpoint any-is-not "none" | endpoints | none identified; unverified candidates: /BSILineEndScale; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Fill | Fills endpoint markers that support open/filled variants. | [false,true] | true | appearance.start-endpoint\|appearance.end-endpoint supports-fill true | endpoints | /LE; unverified candidates: /BSILineEndFill; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Text Color | Color of rendered text. | sRGB/custom | "#000000" | always | text | /DA, /DS; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font | Typeface used for text. | installed fonts | "Helvetica" | always | text | /DA, /DR/Font; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Size | Text size in points. | {"min":1,"max":144,"step":1,"unit":"pt"} | 12 | always | text | /DA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Auto Size | Automatically sizes the text container or font to fit content. | [false,true] | false | always | text/container | /DA; unverified candidates: /BSIAutoSize; AS: typically required; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Line Spacing | Vertical spacing between text baselines. | {"min":0.5,"max":3,"step":0.1,"unit":"×"} | 1 | always | text | /DS; unverified candidates: /BSILineSpacing; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Margin | Inset between text and its container border. | {"min":0,"max":72,"step":0.5,"unit":"pt"} | 3 | always | text/container | /RD; unverified candidates: /BSIMargin; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Alignment | Horizontal and vertical alignment of text within its box. | ["left","center","right","top","middle","bottom"] | "left/top" | always | text | /Q; unverified candidates: /BSIVerticalAlignment; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Font Style | Bold, italic, underline, strikethrough, superscript, and subscript text emphasis. | ["bold","italic","underline","strikethrough","superscript","subscript"] | [] | always | text | /DA, /DS; unverified candidates: /BSIFontStyle; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Cloud Size | Controls the radius/frequency of cloud scallops. | {"min":0.5,"max":10,"step":0.5,"unit":"pt"} | 2 | always | cloud border | /BE/I; unverified candidates: /BSICloudSize; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Invert | Flips cloud scallops inward. | [false,true] | false | appearance.line-style is "cloud" | cloud border | none identified; unverified candidates: /BSICloudInverted; AS: required; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Shape | Chooses the text-box container geometry. | ["rectangle","circle","triangle","hexagon"] | "rectangle" | always | container | /Subtype, /RD; unverified candidates: /BSIShape; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Image

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Image tool](https://support.bluebeam.com/user-manual/menus/tools/image-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Snapshot

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Snapshot tool](https://support.bluebeam.com/user-manual/menus/edit/snapshot.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | X | Horizontal position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Y | Vertical position in page coordinates. | {"step":0.01,"unit":"page units"} | 0 | always | geometry | /Rect, /L, /Vertices; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Width | Horizontal extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Height | Vertical extent of the markup bounding box. | {"min":0,"step":0.01,"unit":"page units"} | "drawn size" | always | geometry | /Rect; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Layout | Rotation | Clockwise markup rotation. | {"min":0,"max":359,"step":1,"unit":"°"} | 0 | always | geometry/content | /Rotate; unverified candidates: /BSIRotation; AS: may carry rotation visually; grouped: not-applicable | preserved-untouched | unverified |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Count (not implemented in Butter Paper)

Sources: [Revu 21 user manual](https://support.bluebeam.com/user-manual/dashboard.html), [Properties panel](https://support.bluebeam.com/user-manual/menus/window/file-properties.html), [Properties toolbar](https://support.bluebeam.com/revu/features/edit-markups-properties-toolbar.html), [Count tool](https://support.bluebeam.com/user-manual/menus/tools/count-tool.html).

| Section | Property | Meaning | Type / values | Default | Condition | Visual component | PDF / appearance stream | Revu → Butter | Butter → Revu |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| General | Subject | Human-readable markup type or purpose shown in the Markups List. | text | "tool label" | always | metadata | /Subj; AS: unverified; grouped: not-applicable | preserved-untouched | exact |
| General | Author | Person attributed as the markup creator. | text | "current Revu user" | always | metadata | /T; AS: unverified; grouped: not-applicable | preserved-untouched | exact |
| General | Comments | Free-form markup comment or text content. | multiline-text | "" | always | text/metadata | /Contents, /RC; AS: unverified; grouped: not-applicable | preserved-untouched | exact |
| General | Locked | Prevents interactive changes until unlocked. | [false,true] | false | always | interaction | /F; AS: unverified; grouped: not-applicable | preserved-untouched | exact |
| General | Label | Display label used by the Markups List and summaries. | text | "" | always | metadata | /Contents, /Subj; unverified candidates: /BSIColumnData; AS: unverified; grouped: not-applicable | preserved-untouched | unverified |
| Appearance | Color | Color of the outline, path, leader, or measurement line. | sRGB/custom/transparent | "#000000" | always | stroke | /C; AS: unverified; grouped: not-applicable | preserved-untouched | exact |
| Appearance | Opacity | Transparency of the stroke and primary visual content. | {"min":0,"max":100,"step":1,"unit":"%"} | 100 | always | stroke/content | /CA; AS: unverified; grouped: not-applicable | preserved-untouched | exact |
| Appearance | Text Color | Color of rendered text. | sRGB/custom | "#000000" | measurement.caption is true | text | /DA, /DS; AS: unverified; grouped: not-applicable | preserved-untouched | normalized |
| Appearance | Font | Typeface used for text. | installed fonts | "Helvetica" | measurement.caption is true | text | /DA, /DR/Font; AS: unverified; grouped: not-applicable | preserved-untouched | normalized |
| Appearance | Font Size | Text size in points. | {"min":1,"max":144,"step":1,"unit":"pt"} | 12 | measurement.caption is true | text | /DA; AS: unverified; grouped: not-applicable | preserved-untouched | exact |
| Appearance | Auto Size | Automatically sizes the text container or font to fit content. | [false,true] | false | measurement.caption is true | text/container | /DA; unverified candidates: /BSIAutoSize; AS: typically required; grouped: not-applicable | preserved-untouched | visual-only |
| Appearance | Line Spacing | Vertical spacing between text baselines. | {"min":0.5,"max":3,"step":0.1,"unit":"×"} | 1 | measurement.caption is true | text | /DS; unverified candidates: /BSILineSpacing; AS: unverified; grouped: not-applicable | preserved-untouched | visual-only |
| Appearance | Margin | Inset between text and its container border. | {"min":0,"max":72,"step":0.5,"unit":"pt"} | 3 | measurement.caption is true | text/container | /RD; unverified candidates: /BSIMargin; AS: unverified; grouped: not-applicable | preserved-untouched | normalized |
| Appearance | Alignment | Horizontal and vertical alignment of text within its box. | ["left","center","right","top","middle","bottom"] | "left/top" | measurement.caption is true | text | /Q; unverified candidates: /BSIVerticalAlignment; AS: unverified; grouped: not-applicable | preserved-untouched | normalized |
| Appearance | Font Style | Bold, italic, underline, strikethrough, superscript, and subscript text emphasis. | ["bold","italic","underline","strikethrough","superscript","subscript"] | [] | measurement.caption is true | text | /DA, /DS; unverified candidates: /BSIFontStyle; AS: unverified; grouped: not-applicable | preserved-untouched | normalized |
| Measurement | Units | Display unit for measured values. | ["page-scale","mm","cm","m","in","ft"] | "page-scale" | always | measurement caption | /Measure/U; AS: unverified; grouped: not-applicable | preserved-untouched | exact |
| Measurement | Precision | Number and fractional precision used for the displayed result. | unit-dependent decimal or fractional precision | "document scale" | always | measurement caption | /Measure/D; AS: unverified; grouped: not-applicable | preserved-untouched | normalized |
| Measurement | Show Caption | Shows the measurement value/caption on the markup. | [false,true] | true | always | measurement caption | /Contents; unverified candidates: /BSICaption; AS: unverified; grouped: not-applicable | preserved-untouched | normalized |
| Measurement | Symbol | Shape used for each Count instance. | Revu Count symbol library | "circle" | always | count instances | /AP; unverified candidates: /BSICountSymbol; AS: required; grouped: instances are associated into a Count group | unsupported | unsupported |
| Measurement | Scale | Uniformly scales Count symbols. | {"min":1,"max":500,"step":1,"unit":"%"} | 100 | always | count instances | /Rect, /AP; unverified candidates: /BSICountScale; AS: required; grouped: applies to every instance in the group | unsupported | unsupported |
| Layout | Width | Width of every Count symbol. | {"min":0,"max":null,"step":0.01,"unit":"page units"} | "symbol default" | always | count instances | /Rect; unverified candidates: /BSICountWidth; AS: required; grouped: applies to the group | unsupported | unsupported |
| Layout | Height | Height of every Count symbol. | {"min":0,"max":null,"step":0.01,"unit":"page units"} | "symbol default" | always | count instances | /Rect; unverified candidates: /BSICountHeight; AS: required; grouped: applies to the group | unsupported | unsupported |
| Measurement | Depth | Depth associated with every uniformly dimensioned Count instance. | {"min":0,"max":null,"step":0.01,"unit":"measurement units"} | 0 | always | count measurement totals | /Measure; unverified candidates: /BSICountDepth; AS: unverified; grouped: applies to the group | unsupported | unsupported |
| Measurement | Group | Logical Count measurement that owns all placed instances and a shared total. | Revu Count group | "new group" | always | all count instances/caption | none identified; unverified candidates: /BSIGroup, /BSICount; AS: required; grouped: fundamental | unsupported | unsupported |
| Options | Set as Default | Uses the current appearance for subsequently created markups of this tool. | action | false | always | future tool defaults | none identified; AS: unverified; grouped: not-applicable | unverified | unsupported |
| Options | Add to Tool Chest | Stores the configured markup as a reusable tool. | action | false | always | future tool preset | none identified; unverified candidates: /BSIToolSet; AS: unverified; grouped: not-applicable | unverified | unsupported |

## Required probe matrix

For each property above, create one-variable-at-a-time specimens covering untouched save, Revu edit and resave, Butter edit and Revu reimport, and conditional transitions. Count additionally requires add/remove/regroup tests; Cloud+ requires both inline and externally associated components.

Same-renderer comparisons isolate producer differences. Interoperability comparisons use fixed PDF hash, page geometry, scale/zoom, ROI, DPI, locale, fonts, background, and theme while allowing the viewer application and OS to differ. Both modes retain registration, SSIM, ink-mask IoU, boundary distance, luminance, continuity, and heatmap evidence.
