//! The `#[tauri::command]`s the UI invokes. Each one runs blocking device work
//! off the main thread and maps core types to DTOs.

use std::sync::Arc;

use bezel_core::app::discover_screens;
use bezel_core::ports::DeviceBus;
use tauri::State;

use crate::dto::ScreenDto;

/// The bus shared by every command.
pub type SharedBus = Arc<dyn DeviceBus + Send + Sync>;

/// State managed by Tauri.
pub struct AppState {
    /// Where screens are discovered.
    pub bus: SharedBus,
}

/// Lists the connected screens on `bus`.
///
/// # Errors
///
/// The core error, as text for the UI.
pub fn screens_on(bus: &(dyn DeviceBus + Send + Sync)) -> Result<Vec<ScreenDto>, String> {
    discover_screens(bus)
        .map(|screens| screens.iter().map(ScreenDto::from).collect())
        .map_err(|e| e.to_string())
}

/// Lists the connected screens (read-only).
///
/// # Errors
///
/// Discovery or task failures, as text.
#[tauri::command]
pub async fn list_screens(state: State<'_, AppState>) -> Result<Vec<ScreenDto>, String> {
    let bus = Arc::clone(&state.bus);
    tauri::async_runtime::spawn_blocking(move || screens_on(bus.as_ref()))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_devices::FakeBus;

    #[test]
    fn screens_on_the_fake_bus() {
        let screens = screens_on(&FakeBus::turing_88()).unwrap();
        assert_eq!(screens.len(), 1);
        assert_eq!(screens[0].models[0].id, "turing-8.8");
        assert!(screens_on(&FakeBus::default()).unwrap().is_empty());
    }
}
