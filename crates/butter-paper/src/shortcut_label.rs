//! Shortcuts as the platform writes them: `Shift+Alt+Q` on Windows and
//! Linux, `⌥⇧Q` on macOS (modifiers in Apple's ⌃⌥⇧⌘ order, no separator).

/// `shortcut` (`Shift+Alt+Q`, `Ctrl+Z`, `Alt+Shift+Left`) for this platform.
pub fn shortcut_label(shortcut: &str) -> String {
    format_shortcut(shortcut, cfg!(target_os = "macos"))
}

/// `shortcut` as a stock key binding, which a tooltip shows beside its text
/// the platform's way.
pub fn shortcut_kbd(shortcut: &str) -> Option<gpui_component::kbd::Kbd> {
    let keystroke = keystroke_text(shortcut);
    gpui::Keystroke::parse(&keystroke)
        .ok()
        .map(gpui_component::kbd::Kbd::new)
}

/// `Shift+Alt+Q` as GPUI writes a keystroke: `shift-alt-q`.
fn keystroke_text(shortcut: &str) -> String {
    shortcut
        .split('+')
        .map(|part| match part {
            "Ctrl" | "Control" => "ctrl".to_owned(),
            "Alt" | "Option" => "alt".to_owned(),
            "Shift" => "shift".to_owned(),
            "Cmd" | "Command" => "cmd".to_owned(),
            "Esc" => "escape".to_owned(),
            other => other.to_ascii_lowercase(),
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn format_shortcut(shortcut: &str, mac: bool) -> String {
    if !mac {
        return shortcut.to_owned();
    }
    let mut modifiers = [false; 4];
    let mut key = String::new();
    for part in shortcut.split('+') {
        match part {
            "Ctrl" | "Control" => modifiers[0] = true,
            "Alt" | "Option" => modifiers[1] = true,
            "Shift" => modifiers[2] = true,
            "Cmd" | "Command" => modifiers[3] = true,
            "Left" => key.push('←'),
            "Right" => key.push('→'),
            "Up" => key.push('↑'),
            "Down" => key.push('↓'),
            "Esc" => key.push('⎋'),
            "Enter" => key.push('↩'),
            other => key.push_str(other),
        }
    }
    let mut label: String = ["⌃", "⌥", "⇧", "⌘"]
        .iter()
        .zip(modifiers)
        .filter_map(|(symbol, held)| held.then_some(*symbol))
        .collect();
    label.push_str(&key);
    label
}

#[cfg(test)]
mod tests {
    use super::{format_shortcut, keystroke_text, shortcut_kbd};

    #[test]
    fn mac_shortcuts_use_apple_symbols_in_apple_order() {
        assert_eq!(format_shortcut("Shift+Alt+Q", true), "⌥⇧Q");
        assert_eq!(format_shortcut("Alt+Shift+Left", true), "⌥⇧←");
        assert_eq!(format_shortcut("Ctrl+Shift+Cmd+Z", true), "⌃⇧⌘Z");
        assert_eq!(format_shortcut("Shift+L", true), "⇧L");
        assert_eq!(format_shortcut("V", true), "V");
    }

    #[test]
    fn other_platforms_keep_the_written_shortcut() {
        assert_eq!(format_shortcut("Shift+Alt+Q", false), "Shift+Alt+Q");
        assert_eq!(format_shortcut("V", false), "V");
    }

    #[test]
    fn shortcuts_become_stock_key_bindings() {
        assert_eq!(keystroke_text("Shift+Alt+Q"), "shift-alt-q");
        assert_eq!(keystroke_text("V"), "v");
        assert_eq!(keystroke_text("Alt+Shift+Left"), "alt-shift-left");
        for shortcut in ["Shift+Alt+Q", "V", "Shift+T", "Ctrl+Z"] {
            assert!(shortcut_kbd(shortcut).is_some(), "{shortcut} parses");
        }
    }
}
