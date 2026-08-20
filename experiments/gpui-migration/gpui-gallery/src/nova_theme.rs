//! Direct translation of the reviewed Butter Paper Nova shell tokens.
//!
//! Source references:
//! - apps/desktop/src/renderer/src/styles.css
//! - apps/desktop/src/renderer/src/components/shellSpacing.ts
//! - apps/desktop/src/renderer/src/components/LeftSidebar.tsx
//!
//! These are product tokens, not GPUI defaults. GPUI supplies the layout and
//! painting primitives that consume them.

pub const BG: u32 = 0xf7f7f7;
pub const SURFACE: u32 = 0xffffff;
pub const BORDER: u32 = 0xe5e5e5;
pub const TEXT: u32 = 0x0a0a0a;
pub const MUTED: u32 = 0x737373;
pub const ACCENT: u32 = 0xf5f5f5;
pub const FOCUS: u32 = 0xa3a3a3;
pub const PAGE: u32 = 0xf2efe5;
pub const VIEWPORT: u32 = 0xf4f4f5;

pub const BODY_FONT_SIZE: f32 = 13.0;
pub const BASE_RADIUS: f32 = 10.0;
pub const CONTROL_RADIUS: f32 = 8.0;
pub const CONTROL_HEIGHT: f32 = 32.0;
pub const CONTROL_ICON_SIZE: f32 = 16.0;
#[allow(dead_code)] // Used when the current SVG icon paths replace text placeholders.
pub const CONTROL_ICON_STROKE_WIDTH: f32 = 1.5;

pub const MENU_BAR_HEIGHT: f32 = 32.0;
pub const WINDOW_TITLE_BAR_HEIGHT: f32 = 32.0;
pub const PRIMARY_BAND_HEIGHT: f32 = 48.0;
pub const TAB_HEIGHT: f32 = 32.0;
pub const DOCUMENT_TAB_BAR_HEIGHT: f32 = 48.0;
pub const RAIL_WIDTH: f32 = 48.0;
pub const RIGHT_RAIL_WIDTH: f32 = 88.0;
pub const RAIL_BUTTON_SIZE: f32 = 32.0;
pub const RAIL_BUTTON_GAP: f32 = 8.0;
pub const SIDEBAR_WIDTH: f32 = 300.0;
