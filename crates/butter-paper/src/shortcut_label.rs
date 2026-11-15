//! Shortcuts as the platform writes them: `Shift+Alt+Q` on Windows and
//! Linux, `⌥⇧Q` on macOS (modifiers in Apple's ⌃⌥⇧⌘ order, no separator).

/// `shortcut` (`Shift+Alt+Q`, `Ctrl+Z`, `Alt+Shift+Left`) for this platform.
pub fn shortcut_label(shortcut: &str) -> String {
    format_shortcut(shortcut, cfg!(target_os = "macos"))
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
    use super::format_shortcut;

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
}
