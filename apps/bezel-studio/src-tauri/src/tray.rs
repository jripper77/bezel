//! The tray icon: Bezel keeps running there while a screen is live. Its
//! menu opens or hides the window, turns live mode on or off (the check
//! mark follows the session, whoever changed it) and quits. Its labels
//! follow the app's language.

use std::sync::{Arc, Mutex, PoisonError};

use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, MenuBuilder, MenuItem, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter as _, Manager};

use crate::clock::now;
use crate::commands::{Shared, Unsaved};
use crate::diag::{self, DiagCode};
use crate::texts::Texts;

const SHOW: &str = "show";
const HIDE: &str = "hide";
const LIVE: &str = "live";
const QUIT: &str = "quit";

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
            Err(_) => diag::report(DiagCode::TrayLiveItemNotUpdated),
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
        && window.hide().is_err()
    {
        diag::report(DiagCode::WindowNotHidden);
    }
}

/// Quits, unless edits are unsaved: then the window shows and the UI asks
/// first (studio-app review W2). A UI that cannot be asked does not keep
/// the app running.
fn quit(app: &AppHandle) {
    let unsaved = app.try_state::<Unsaved>().is_some_and(|u| u.get());
    match crate::on_quit(unsaved) {
        crate::OnQuit::Exit => app.exit(0),
        crate::OnQuit::Ask => {
            crate::show_main_window(app);
            if app.emit(crate::QUIT_EVENT, ()).is_err() {
                diag::report(DiagCode::UnsavedEditsNotAskedBeforeQuitting);
                app.exit(0);
            }
        }
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
            if backend.toggle_live(now()).is_err() {
                diag::report(DiagCode::TrayLiveFailed);
            }
            // Not under the session's lock: the menu waits for the main thread.
            let live = backend.studio().live_key().is_some();
            item.sync(live);
        });
    if spawned.is_err() {
        diag::report(DiagCode::TrayLiveNotStarted);
    }
}

/// The menu's items, to label them again in another language.
#[derive(Clone)]
pub(crate) struct TrayMenu {
    show: MenuItem<tauri::Wry>,
    hide: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
    live: LiveItem,
}

impl TrayMenu {
    /// The live check item, which follows the session.
    pub(crate) fn live(&self) -> &LiveItem {
        &self.live
    }

    /// Labels the menu with `text`.
    pub(crate) fn relabel(&self, text: &Texts) {
        let labels = [
            self.show.set_text(text.show),
            self.hide.set_text(text.hide),
            self.live.item.set_text(text.live),
            self.quit.set_text(text.quit),
        ];
        for result in labels {
            if result.is_err() {
                diag::report(DiagCode::TrayMenuNotRelabelled);
            }
        }
    }
}

/// Adds the tray icon with its menu in `text`; `live` is whether a screen
/// is live now.
pub(crate) fn create(app: &AppHandle, live: bool, text: &Texts) -> tauri::Result<TrayMenu> {
    let live_item = LiveItem {
        item: CheckMenuItemBuilder::with_id(LIVE, text.live)
            .checked(live)
            .build(app)?,
        shown: Arc::new(Mutex::new(Some(live))),
    };
    let items = TrayMenu {
        show: MenuItemBuilder::with_id(SHOW, text.show).build(app)?,
        hide: MenuItemBuilder::with_id(HIDE, text.hide).build(app)?,
        quit: MenuItemBuilder::with_id(QUIT, text.quit).build(app)?,
        live: live_item.clone(),
    };
    let menu = MenuBuilder::new(app)
        .item(&items.show)
        .item(&items.hide)
        .separator()
        .item(&live_item.item)
        .separator()
        .item(&items.quit)
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
            QUIT => quit(app),
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
    Ok(items)
}
