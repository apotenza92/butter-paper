//! Butter Paper-owned controls built directly on GPUI.
//!
//! The visible contract comes from the production Nova renderer. This module
//! must not depend on `gpui-component` or Zed's GPL-licensed `ui` crate.

mod button;
mod button_group;
mod icon;
mod menu;
mod separator;
mod split_button;
mod theme;
mod tooltip;

pub use button::{Button, ButtonGroupPosition, ButtonSize, ButtonVariant};
pub use button_group::ButtonGroup;
pub use icon::Icon;
pub use menu::{PopupMenu, PopupMenuItem};
pub use separator::{Separator, SeparatorOrientation};
pub use split_button::SplitButton;
pub use theme::ButterTheme;
pub use tooltip::Tooltip;
