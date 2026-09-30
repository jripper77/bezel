//! `bezel devices`: human table and JSON output.

use bezel_core::app::discover_screens;
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::{Endpoint, Screen, ScreenState};
use bezel_core::ports::DeviceBus;
use serde::Serialize;
use std::fmt::Write as _;

/// JSON shape of one screen. Field names are a public contract of the CLI.
#[derive(Debug, Serialize)]
struct ScreenDto {
    state: &'static str,
    family: &'static str,
    models: Vec<ModelDto>,
    display: Option<EndpointDto>,
    wake: Option<EndpointDto>,
}

#[derive(Debug, Serialize)]
struct ModelDto {
    id: &'static str,
    name: &'static str,
    diagonal: String,
    width: u32,
    height: u32,
    hardware_validated: bool,
}

#[derive(Debug, Serialize)]
struct EndpointDto {
    address: String,
    usb: String,
    serial: Option<String>,
    manufacturer: Option<String>,
    product: Option<String>,
    location: Option<String>,
}

fn state_name(state: ScreenState) -> &'static str {
    match state {
        ScreenState::Awake => "awake",
        ScreenState::Asleep => "asleep",
    }
}

fn model_dto(m: &DeviceModel) -> ModelDto {
    ModelDto {
        id: m.id.0,
        name: m.name,
        diagonal: m.diagonal(),
        width: m.panel.width,
        height: m.panel.height,
        hardware_validated: m.hardware_validated,
    }
}

fn endpoint_dto(e: &Endpoint) -> EndpointDto {
    EndpointDto {
        address: e.address.0.clone(),
        usb: e.usb.to_string(),
        serial: e.serial_number.clone(),
        manufacturer: e.manufacturer.clone(),
        product: e.product.clone(),
        location: e.location.as_ref().map(ToString::to_string),
    }
}

fn screen_dto(s: &Screen) -> ScreenDto {
    ScreenDto {
        state: state_name(s.state()),
        family: s.family.slug(),
        models: s.candidates.iter().map(|m| model_dto(m)).collect(),
        display: s.display.as_ref().map(endpoint_dto),
        wake: s.wake.as_ref().map(endpoint_dto),
    }
}

/// Discovers screens on `bus` and renders them.
pub fn list<B: DeviceBus + ?Sized>(bus: &B, json: bool) -> anyhow::Result<String> {
    let screens = discover_screens(bus)?;
    if json {
        let dtos: Vec<ScreenDto> = screens.iter().map(screen_dto).collect();
        return Ok(serde_json::to_string_pretty(&dtos)? + "\n");
    }
    Ok(table(&screens))
}

fn title(s: &Screen) -> String {
    match s.model() {
        Some(m) => format!("{}  {}x{}", m.name, m.panel.width, m.panel.height),
        None => {
            let names: Vec<&str> = s.candidates.iter().map(|m| m.name).collect();
            format!("{} (exact model confirmed on connect)", names.join(" / "))
        }
    }
}

fn endpoint_line(out: &mut String, label: &str, e: &Endpoint) {
    let serial = e.serial_number.as_deref().unwrap_or("-");
    let location = e
        .location
        .as_ref()
        .map_or_else(|| "-".to_string(), ToString::to_string);
    let _ = writeln!(
        out,
        "   {label:<8} {:<14} {}  serial {serial}  usb {location}",
        e.address, e.usb
    );
}

fn table(screens: &[Screen]) -> String {
    if screens.is_empty() {
        return "No smart screen found.\n\
                Check the USB cable, and on Linux that your user may open the port \
                (packaging/linux/60-bezel.rules grants it).\n"
            .to_string();
    }
    let mut out = String::new();
    for (i, s) in screens.iter().enumerate() {
        let _ = writeln!(out, "{}. {}  [{}]", i + 1, title(s), state_name(s.state()));
        let _ = writeln!(out, "   {:<8} {}", "family", s.family.slug());
        if let Some(d) = &s.display {
            endpoint_line(&mut out, "display", d);
        }
        if let Some(w) = &s.wake {
            endpoint_line(&mut out, "wake", w);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_devices::FakeBus;

    #[test]
    fn table_shows_one_turing_88() {
        let out = list(&FakeBus::turing_88(), false).unwrap();
        assert!(
            out.starts_with("1. Turing Smart Screen 8.8\"  480x1920  [awake]"),
            "{out}"
        );
        assert!(out.contains("display  /dev/ttyACM1"), "{out}");
        assert!(out.contains("wake     /dev/ttyACM0"), "{out}");
        assert!(out.contains("serial CT88INCH  usb 3-1.1"), "{out}");
    }

    #[test]
    fn empty_bus_explains_what_to_check() {
        let out = list(&FakeBus::default(), false).unwrap();
        assert!(out.starts_with("No smart screen found."));
        assert_eq!(list(&FakeBus::default(), true).unwrap(), "[]\n");
    }
}
