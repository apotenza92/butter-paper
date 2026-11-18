//! Follows the operating system's reduce-motion setting.
//!
//! GPUI and gpui-component play every animation to its end state at once
//! while `App::reduce_motion` is set, but GPUI does not read the system's
//! setting. This module reads it when the app starts and follows changes:
//!
//! - macOS: System Settings › Accessibility › Display › Reduce motion.
//! - Windows: Settings › Accessibility › Visual effects › Animation effects.
//! - Linux: the desktop portal's `reduced-motion`, GNOME's Reduce animation
//!   (`enable-animations`) and KDE's animation speed.

use gpui::App;

/// Sets GPUI's reduce-motion flag from the system and keeps it current.
pub fn follow_system(cx: &mut App) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        // GPUI owns the platform's notifications, so the setting is read
        // again on a short timer; the read is a single cheap system call.
        const POLL: std::time::Duration = std::time::Duration::from_secs(2);
        cx.set_reduce_motion(system_reduces_motion());
        cx.spawn(async move |cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let reduce = system_reduces_motion();
                cx.update(|cx| cx.set_reduce_motion(reduce));
            }
        })
        .detach();
    }
    #[cfg(target_os = "linux")]
    linux::follow(cx);
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    let _ = cx;
}

#[cfg(target_os = "macos")]
// objc 0.2's macros test a `cargo-clippy` feature this crate does not have.
#[allow(unexpected_cfgs)]
fn system_reduces_motion() -> bool {
    use objc::runtime::{NO, Object};
    use objc::{class, msg_send, sel, sel_impl};
    // SAFETY: `+[NSWorkspace sharedWorkspace]` and its
    // `accessibilityDisplayShouldReduceMotion` property are available since
    // macOS 10.12 and are read on the main thread here.
    unsafe {
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let reduce: objc::runtime::BOOL =
            msg_send![workspace, accessibilityDisplayShouldReduceMotion];
        reduce != NO
    }
}

#[cfg(target_os = "windows")]
fn system_reduces_motion() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut animate: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer.
    let read = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut animate as *mut i32).cast(),
            0,
        )
    };
    read != 0 && animate == 0
}

/// Whether the Linux desktop asks for reduced motion. Any one source that
/// asks for it is enough: the portal's appearance key, GNOME's switch
/// (`enable-animations` off) or KDE's animation speed set to instant.
pub fn linux_reduces_motion(
    portal_reduced: Option<bool>,
    gnome_animations: Option<bool>,
    kde_duration_factor: Option<f64>,
) -> bool {
    portal_reduced == Some(true)
        || gnome_animations == Some(false)
        || kde_duration_factor.is_some_and(|factor| factor <= 0.)
}

#[cfg(target_os = "linux")]
mod linux {
    use ashpd::desktop::settings::{ReducedMotion, Settings};
    use futures_lite::StreamExt as _;
    use gpui::App;

    const GNOME: (&str, &str) = ("org.gnome.desktop.interface", "enable-animations");
    const KDE: (&str, &str) = ("org.kde.kdeglobals.KDE", "AnimationDurationFactor");

    async fn read(settings: &Settings) -> bool {
        let portal = settings
            .reduced_motion()
            .await
            .ok()
            .map(|motion| motion == ReducedMotion::ReducedMotion);
        let gnome = settings.read::<bool>(GNOME.0, GNOME.1).await.ok();
        let kde = settings.read::<f64>(KDE.0, KDE.1).await.ok();
        super::linux_reduces_motion(portal, gnome, kde)
    }

    pub(super) fn follow(cx: &mut App) {
        let (sender, receiver) = async_channel::unbounded::<bool>();
        // The portal is a D-Bus service: talk to it off the main thread and
        // hand each reading to the app.
        cx.background_executor()
            .spawn(async move {
                let Ok(settings) = Settings::new().await else {
                    return;
                };
                if sender.send(read(&settings).await).await.is_err() {
                    return;
                }
                let Ok(mut changes) = settings.receive_setting_changed().await else {
                    return;
                };
                while let Some(change) = changes.next().await {
                    let watched = [("org.freedesktop.appearance", "reduced-motion"), GNOME, KDE];
                    if watched.contains(&(change.namespace(), change.key()))
                        && sender.send(read(&settings).await).await.is_err()
                    {
                        return;
                    }
                }
            })
            .detach();
        cx.spawn(async move |cx| {
            while let Ok(reduce) = receiver.recv().await {
                cx.update(|cx| cx.set_reduce_motion(reduce));
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::linux_reduces_motion;

    #[test]
    fn any_linux_source_that_asks_for_reduced_motion_is_enough() {
        assert!(!linux_reduces_motion(None, None, None));
        assert!(!linux_reduces_motion(Some(false), Some(true), Some(1.)));
        assert!(linux_reduces_motion(Some(true), None, None));
        assert!(linux_reduces_motion(None, Some(false), None));
        assert!(linux_reduces_motion(None, None, Some(0.)));
        assert!(!linux_reduces_motion(None, None, Some(0.5)));
    }
}
