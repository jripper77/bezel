//! The tray icon: Bezel keeps running there while a screen is live.

use tauri::AppHandle;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::clock::language;
use bezel_core::domain::clock::Language;

const SHOW: &str = "show";
const QUIT: &str = "quit";

/// Menu labels in the user's language.
fn labels(lang: Language) -> (&'static str, &'static str) {
    match lang {
        Language::PortugueseBr => ("Abrir o Bezel", "Sair"),
        Language::English => ("Open Bezel", "Quit"),
    }
}

/// Adds the tray icon with its menu.
pub(crate) fn create(app: &AppHandle) -> tauri::Result<()> {
    let (show, quit) = labels(language());
    let menu = MenuBuilder::new(app)
        .item(&MenuItemBuilder::with_id(SHOW, show).build(app)?)
        .separator()
        .item(&MenuItemBuilder::with_id(QUIT, quit).build(app)?)
        .build()?;
    let mut builder = TrayIconBuilder::with_id("bezel")
        .tooltip("Bezel")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            SHOW => crate::show_main_window(app),
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_language() {
        assert_eq!(labels(Language::PortugueseBr).1, "Sair");
        assert_eq!(labels(Language::English).0, "Open Bezel");
    }
}
