//! The tray icon: Bezel keeps running there while a screen is live. Its
//! menu opens or hides the window, turns live mode on or off (the check
//! mark follows the session, whoever changed it) and quits.

use std::sync::{Arc, Mutex, PoisonError};

use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::clock::{language, now};
use crate::commands::Shared;
use bezel_core::domain::clock::Language;

const SHOW: &str = "show";
const HIDE: &str = "hide";
const LIVE: &str = "live";
const QUIT: &str = "quit";

/// Menu labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Labels {
    show: &'static str,
    hide: &'static str,
    live: &'static str,
    quit: &'static str,
}

/// Menu labels in the user's language.
fn labels(lang: Language) -> Labels {
    match lang {
        Language::PortugueseBr => Labels {
            show: "Abrir o Bezel",
            hide: "Ocultar a janela",
            live: "Ao vivo na tela",
            quit: "Sair",
        },
        Language::English => Labels {
            show: "Open Bezel",
            hide: "Hide the window",
            live: "Live on the screen",
            quit: "Quit",
        },
    }
}

/// The menu's live check item, kept in step with the session.
#[derive(Clone)]
pub(crate) struct LiveItem {
    item: CheckMenuItem<tauri::Wry>,
    /// What the check mark shows (`None` once a click toggled it).
    shown: Arc<Mutex<Option<bool>>>,
}

impl LiveItem {
    /// Shows whether a screen is `live` (the menu is touched only when it
    /// changes).
    pub(crate) fn sync(&self, live: bool) {
        let mut shown = self.shown.lock().unwrap_or_else(PoisonError::into_inner);
        if *shown == Some(live) {
            return;
        }
        match self.item.set_checked(live) {
            Ok(()) => *shown = Some(live),
            Err(e) => tracing::warn!("tray live item not updated: {e}"),
        }
    }

    /// A click toggled the check mark by itself: it is unknown until the
    /// next [`Self::sync`].
    fn clicked(&self) {
        *self.shown.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}

/// Hides the main window (Bezel stays in the tray).
fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(crate::MAIN_WINDOW)
        && let Err(e) = window.hide()
    {
        tracing::warn!("window not hidden: {e}");
    }
}

/// Turns live mode on or off from the menu, off the main thread (opening a
/// screen takes seconds), then shows the outcome.
fn toggle_live(app: &AppHandle, item: &LiveItem) {
    item.clicked();
    let Some(backend) = app.try_state::<Shared>().map(|b| Arc::clone(&b)) else {
        return;
    };
    let item = item.clone();
    let spawned = std::thread::Builder::new()
        .name("bezel-tray-live".into())
        .spawn(move || {
            if let Err(e) = backend.toggle_live(now()) {
                tracing::warn!("live mode from the tray: {e}");
            }
            // Not under the session's lock: the menu waits for the main thread.
            let live = backend.studio().live_key().is_some();
            item.sync(live);
        });
    if let Err(e) = spawned {
        tracing::warn!("live mode from the tray not started: {e}");
    }
}

/// Adds the tray icon with its menu; `live` is whether a screen is live now.
pub(crate) fn create(app: &AppHandle, live: bool) -> tauri::Result<LiveItem> {
    let text = labels(language());
    let live_item = LiveItem {
        item: CheckMenuItemBuilder::with_id(LIVE, text.live)
            .checked(live)
            .build(app)?,
        shown: Arc::new(Mutex::new(Some(live))),
    };
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id(SHOW, text.show).build(app)?)
        .item(&MenuItemBuilder::with_id(HIDE, text.hide).build(app)?)
        .separator()
        .item(&live_item.item)
        .separator()
        .item(&MenuItemBuilder::with_id(QUIT, text.quit).build(app)?)
        .build()?;
    let clicked = live_item.clone();
    let mut builder = TrayIconBuilder::with_id("bezel")
        .tooltip("Bezel")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            SHOW => crate::show_main_window(app),
            HIDE => hide_main_window(app),
            LIVE => toggle_live(app, &clicked),
            QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(live_item)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_language() {
        let pt = labels(Language::PortugueseBr);
        assert_eq!((pt.show, pt.quit), ("Abrir o Bezel", "Sair"));
        assert_eq!((pt.hide, pt.live), ("Ocultar a janela", "Ao vivo na tela"));
        let en = labels(Language::English);
        assert_eq!(
            (en.show, en.hide, en.live, en.quit),
            (
                "Open Bezel",
                "Hide the window",
                "Live on the screen",
                "Quit"
            )
        );
    }
}
