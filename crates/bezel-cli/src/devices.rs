//! `bezel devices`: human table and JSON output; `bezel monitor-mode`:
//! the switch of a panel in desktop mode back to USB monitor mode.

use std::io::Write;

use anyhow::Context;
use bezel_core::BezelError;
use bezel_core::app::{discover_devices, leave_desktop_mode};
use bezel_core::domain::device::DeviceModel;
use bezel_core::domain::discovery::{
    DesktopModePanel, Discovery, Endpoint, MonitorModeSwitch, Screen, ScreenState,
};
use bezel_core::domain::screen::Confirm;
use bezel_core::ports::{DesktopModeHid, DeviceBus};
use serde::Serialize;

use crate::messages::Messages;

/// JSON shape of one screen, or of one panel in desktop mode (`state`
/// `desktop-mode`, with `hid` and `hardware_validated`; screens leave both
/// out). Field names are a public contract of the CLI.
#[derive(Debug, Serialize)]
struct ScreenDto {
    state: &'static str,
    family: &'static str,
    models: Vec<ModelDto>,
    display: Option<EndpointDto>,
    wake: Option<EndpointDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hid: Option<EndpointDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hardware_validated: Option<bool>,
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

/// The `state` of a panel in desktop mode.
const DESKTOP_MODE: &str = "desktop-mode";

/// How every surface labels desktop mode (D-2026-09-30-release-polish-8).
const NOT_VALIDATED: &str = "not validated on hardware";

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
        hid: None,
        hardware_validated: None,
    }
}

fn desktop_mode_dto(p: &DesktopModePanel) -> ScreenDto {
    ScreenDto {
        state: DESKTOP_MODE,
        family: DesktopModePanel::FAMILY.slug(),
        models: p.candidates.iter().map(|m| model_dto(m)).collect(),
        display: None,
        wake: None,
        hid: Some(endpoint_dto(&p.hid)),
        hardware_validated: Some(DesktopModePanel::HARDWARE_VALIDATED),
    }
}

/// Discovers screens and panels in desktop mode on `bus` and renders them:
/// the screens first, then the panels.
pub fn list<B: DeviceBus + ?Sized>(bus: &B, json: bool) -> anyhow::Result<String> {
    let found = discover_devices(bus)?;
    if json {
        let dtos: Vec<ScreenDto> = found
            .screens
            .iter()
            .map(screen_dto)
            .chain(found.desktop_mode.iter().map(desktop_mode_dto))
            .collect();
        return Ok(serde_json::to_string_pretty(&dtos)? + "\n");
    }
    Ok(table(&found))
}

fn names(models: &[&'static DeviceModel]) -> String {
    let names: Vec<&str> = models.iter().map(|m| m.name).collect();
    names.join(" / ")
}

fn title(s: &Screen) -> String {
    match s.model() {
        Some(m) => format!("{}  {}x{}", m.name, m.panel.width, m.panel.height),
        None => format!(
            "{} (exact model confirmed on connect)",
            names(&s.candidates)
        ),
    }
}

fn endpoint_line(out: &mut String, label: &str, e: &Endpoint) {
    let serial = e.serial_number.as_deref().unwrap_or("-");
    let location = e
        .location
        .as_ref()
        .map_or_else(|| "-".to_string(), ToString::to_string);
    out.push_str(&format!(
        "   {label:<8} {:<14} {}  serial {serial}  usb {location}\n",
        e.address, e.usb
    ));
}

fn table(found: &Discovery) -> String {
    if found.screens.is_empty() && found.desktop_mode.is_empty() {
        return "No smart screen found.\n\
                Check the USB cable, and on Linux that your user may open the port \
                (`bezel udev-rules` prints the udev rule that grants it).\n"
            .to_string();
    }
    let mut out = String::new();
    let screens = &found.screens;
    for (i, s) in screens.iter().enumerate() {
        out.push_str(&format!(
            "{}. {}  [{}]\n",
            i + 1,
            title(s),
            state_name(s.state())
        ));
        out.push_str(&format!("   {:<8} {}\n", "family", s.family.slug()));
        if let Some(d) = &s.display {
            endpoint_line(&mut out, "display", d);
        }
        if let Some(w) = &s.wake {
            endpoint_line(&mut out, "wake", w);
        }
    }
    for (i, p) in found.desktop_mode.iter().enumerate() {
        desktop_mode_entry(&mut out, screens.len() + i + 1, p);
    }
    out
}

fn desktop_mode_entry(out: &mut String, number: usize, p: &DesktopModePanel) {
    out.push_str(&format!(
        "{number}. {} in desktop mode  [{DESKTOP_MODE}, {NOT_VALIDATED}]\n",
        names(&p.candidates)
    ));
    out.push_str(&format!(
        "   {:<8} {}\n",
        "family",
        DesktopModePanel::FAMILY.slug()
    ));
    endpoint_line(out, "hid", &p.hid);
    out.push_str(&format!(
        "   back to USB monitor mode ({NOT_VALIDATED}): bezel monitor-mode --screen {} --yes\n",
        p.address()
    ));
}

/// `bezel monitor-mode`: says what it is about to do on `log`, then
/// switches the panel back through the core use case, which refuses
/// without `--yes` before touching the bus or the panel.
pub fn monitor_mode<B, H>(
    bus: &B,
    hid: &H,
    address: Option<&str>,
    yes: bool,
    log: &mut dyn Write,
) -> anyhow::Result<String>
where
    B: DeviceBus + ?Sized,
    H: DesktopModeHid + ?Sized,
{
    let mut log = Messages::new(log);
    writeln!(
        log,
        "Switch a Turing USB panel in desktop mode back to USB monitor mode ({NOT_VALIDATED})."
    );
    log.check()?;
    let confirm = if yes { Confirm::Yes } else { Confirm::No };
    match leave_desktop_mode(bus, hid, address, confirm) {
        Ok(done) => Ok(switched(&done)),
        Err(BezelError::NotConfirmed(_)) => {
            writeln!(
                log,
                "Nothing was sent to the panel. Add --yes to switch it."
            );
            log.check()?;
            anyhow::bail!("switching back to USB monitor mode needs --yes")
        }
        Err(e) => Err(e).context("could not switch the panel back to USB monitor mode"),
    }
}

fn switched(done: &MonitorModeSwitch) -> String {
    let model = match (done.model(), done.model_byte) {
        (Some(m), _) => format!("it said it is a {}", m.name),
        (None, Some(byte)) => format!("it answered an unknown model byte 0x{byte:02x}"),
        (None, None) => "it did not say its model".to_string(),
    };
    format!(
        "Sent the switch back to USB monitor mode to {} ({model}).\n\
         The panel restarts as a USB screen: `bezel devices` lists it in a few seconds. \
         This is {NOT_VALIDATED}: if it stays in desktop mode, switch it back from the \
         vendor app.\n",
        done.panel.address()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_devices::{FakeBus, FakeHid};

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
        assert!(out.contains("`bezel udev-rules`"), "{out}");
        assert_eq!(list(&FakeBus::default(), true).unwrap(), "[]\n");
    }

    #[test]
    fn desktop_mode_panels_follow_the_screens_labelled_not_validated() {
        let bus = FakeBus::turing_88().and(FakeBus::desktop_mode());
        let out = list(&bus, false).unwrap();
        assert!(out.starts_with("1. Turing Smart Screen 8.8\""), "{out}");
        assert!(
            out.contains(
                "2. Turing 8.8\" V1.x (USB) / Turing 8\" (USB) / Turing 5.2\" (USB) in desktop \
                 mode  [desktop-mode, not validated on hardware]"
            ),
            "{out}"
        );
        assert!(
            out.contains("   hid      hid:/dev/hidraw7 1a86:ad11"),
            "{out}"
        );
        assert!(
            out.contains("bezel monitor-mode --screen hid:/dev/hidraw7 --yes"),
            "{out}"
        );
        let alone = list(&FakeBus::desktop_mode(), false).unwrap();
        assert!(alone.starts_with("1. Turing 8.8\" V1.x (USB)"), "{alone}");

        let json: serde_json::Value = serde_json::from_str(&list(&bus, true).unwrap()).unwrap();
        let screen = &json[0];
        assert!(screen.get("hid").is_none(), "screens keep their shape");
        assert!(screen.get("hardware_validated").is_none());
        let panel = &json[1];
        assert_eq!(panel["state"], "desktop-mode");
        assert_eq!(panel["family"], "turing-usb");
        assert_eq!(panel["display"], serde_json::Value::Null);
        assert_eq!(panel["hid"]["address"], "hid:/dev/hidraw7");
        assert_eq!(panel["hid"]["usb"], "1a86:ad11");
        assert_eq!(panel["hardware_validated"], false);
        assert_eq!(panel["models"].as_array().map(Vec::len), Some(3));
        assert_eq!(panel["models"][0]["id"], "turing-usb-8.8");
    }

    fn monitor_mode_with(
        hid: &FakeHid,
        address: Option<&str>,
        yes: bool,
    ) -> (anyhow::Result<String>, String) {
        let bus = FakeBus::turing_88().and(FakeBus::desktop_mode());
        let mut log = Vec::new();
        let out = monitor_mode(&bus, hid, address, yes, &mut log);
        (out, String::from_utf8(log).unwrap())
    }

    /// A bus that counts how often it is enumerated.
    struct CountingBus {
        inner: FakeBus,
        calls: std::cell::Cell<usize>,
    }

    impl DeviceBus for CountingBus {
        fn endpoints(&self) -> bezel_core::Result<Vec<Endpoint>> {
            self.calls.set(self.calls.get() + 1);
            self.inner.endpoints()
        }
    }

    #[test]
    fn hid_desktop_requires_confirm() {
        let hid = FakeHid::answering(0x88);
        let bus = CountingBus {
            inner: FakeBus::desktop_mode(),
            calls: std::cell::Cell::new(0),
        };
        let mut log = Vec::new();
        let err = monitor_mode(&bus, &hid, None, false, &mut log).unwrap_err();
        assert_eq!(
            err.to_string(),
            "switching back to USB monitor mode needs --yes"
        );
        let log = String::from_utf8(log).unwrap();
        assert!(log.contains("(not validated on hardware)"), "{log}");
        assert!(
            log.contains("Nothing was sent to the panel. Add --yes"),
            "{log}"
        );
        assert!(hid.calls().is_empty(), "no report without --yes");
        assert_eq!(bus.calls.get(), 0, "not even enumerated");

        let (out, _) = monitor_mode_with(&hid, None, true);
        let out = out.unwrap();
        assert!(
            out.starts_with(
                "Sent the switch back to USB monitor mode to hid:/dev/hidraw7 \
                 (it said it is a Turing 8.8\" V1.x (USB))."
            ),
            "{out}"
        );
        assert!(out.contains("not validated on hardware"), "{out}");
        let calls = hid.calls();
        let [first, second] = bezel_devices::hid_desktop::back_to_monitor_reports();
        assert_eq!(
            calls[0].reports,
            vec![bezel_devices::hid_desktop::model_query()]
        );
        assert_eq!(calls[1].reports, vec![first, second]);
    }

    #[test]
    fn monitor_mode_reports_what_the_panel_said_or_why_it_failed() {
        let (out, _) = monitor_mode_with(&FakeHid::silent(), Some("hid:/dev/hidraw7"), true);
        assert!(out.unwrap().contains("(it did not say its model)"));
        let (out, _) = monitor_mode_with(&FakeHid::answering(0x42), None, true);
        assert!(
            out.unwrap()
                .contains("(it answered an unknown model byte 0x42)")
        );

        let hid = FakeHid::silent();
        let (out, _) = monitor_mode_with(&hid, Some("hid:/dev/hidraw1"), true);
        let err = format!("{:#}", out.unwrap_err());
        assert_eq!(
            err,
            "could not switch the panel back to USB monitor mode: \
             screen not found: no panel in desktop mode at hid:/dev/hidraw1"
        );
        let mut log = Vec::new();
        let err = monitor_mode(&FakeBus::turing_88(), &hid, None, true, &mut log).unwrap_err();
        assert!(format!("{err:#}").contains("no panel in desktop mode connected"));
        let two = FakeBus::desktop_mode().and(FakeBus::desktop_mode());
        let err = monitor_mode(&two, &hid, None, true, &mut log).unwrap_err();
        assert!(
            format!("{err:#}").contains("2 panels are in desktop mode"),
            "{err:#}"
        );
        assert!(
            hid.calls().is_empty(),
            "nothing sent when no panel was chosen"
        );
    }
}
