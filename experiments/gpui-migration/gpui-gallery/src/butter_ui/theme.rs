use gpui::{Hsla, rgb, rgba};
use palette::IntoColor as _;

/// Resolved light-theme values from the production `base-nova` token set.
///
/// The source remains `apps/desktop/src/renderer/src/styles.css`. Keeping the
/// resolved values together makes pixel review explicit and prevents controls
/// from inventing local shades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButterTheme {
    pub background: Hsla,
    pub foreground: Hsla,
    pub primary: Hsla,
    pub primary_foreground: Hsla,
    pub secondary: Hsla,
    pub secondary_foreground: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub destructive: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub ring: Hsla,
    pub transparent: Hsla,
}

impl ButterTheme {
    pub fn light() -> Self {
        Self {
            background: rgb(0xffffff).into_color(),
            foreground: rgb(0x0a0a0a).into_color(),
            primary: rgb(0x262626).into_color(),
            primary_foreground: rgb(0xfafafa).into_color(),
            secondary: rgb(0xf5f5f5).into_color(),
            secondary_foreground: rgb(0x262626).into_color(),
            muted: rgb(0xf5f5f5).into_color(),
            muted_foreground: rgb(0x737373).into_color(),
            destructive: rgb(0xe7000b).into_color(),
            border: rgb(0xe5e5e5).into_color(),
            input: rgb(0xe5e5e5).into_color(),
            ring: rgb(0xa3a3a3).into_color(),
            transparent: rgba(0x00000000).into_color(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_tokens_match_the_reviewed_nova_neutrals() {
        let theme = ButterTheme::light();
        assert_eq!(theme.background, rgb(0xffffff).into_color());
        assert_eq!(theme.foreground, rgb(0x0a0a0a).into_color());
        assert_eq!(theme.muted, rgb(0xf5f5f5).into_color());
        assert_eq!(theme.border, rgb(0xe5e5e5).into_color());
        assert_eq!(theme.ring, rgb(0xa3a3a3).into_color());
    }
}
