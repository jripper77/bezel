//! `bezel sensors`: every sensor of the machine as a table grouped by
//! category, or as JSON.
//!
//! Rates (network, disk) and usages (CPU) are deltas between two samples, so
//! the command samples once, waits [`WARM_UP`], and prints the second
//! sample: its values cover those 250 ms. `--watch` keeps sampling at the
//! given interval, each value covering the time since the previous line.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use bezel_core::domain::sensor::{
    Category, DisplayFormat, Reading, SensorInfo, Snapshot, format_reading,
};
use bezel_core::domain::theme::MIN_REFRESH_SECONDS;
use bezel_core::ports::SensorSource;
use serde::Serialize;

use crate::SensorsArgs;

/// Time between the warm-up sample and the first printed one.
pub const WARM_UP: Duration = Duration::from_millis(250);

/// How `--watch` shows successive tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchStyle {
    /// Clear the terminal and redraw (interactive terminals).
    Redraw,
    /// Print one after the other (pipes, files).
    Append,
}

/// Table title of each category.
fn title(category: Category) -> &'static str {
    match category {
        Category::Cpu => "CPU",
        Category::Gpu => "GPU",
        Category::Memory => "Memory",
        Category::Disk => "Disks",
        Category::Network => "Network",
        Category::Board => "Board",
        Category::System => "System",
    }
}

/// JSON shape of one sample. Field names are a public contract of the CLI.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SampleDto<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    sample_millis: Option<u64>,
    sensors: Vec<SensorDto<'a>>,
}

/// One sensor: exactly one of `value`, `text` and `unavailable` is present.
#[derive(Debug, Serialize)]
struct SensorDto<'a> {
    key: &'a str,
    category: &'static str,
    label: &'a str,
    quantity: &'static str,
    source: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unavailable: Option<String>,
}

fn sensor_dto<'a>(info: &'a SensorInfo, reading: Reading) -> SensorDto<'a> {
    let (value, text, unavailable) = match reading {
        Reading::Value(v) if v.is_finite() => (Some(v), None, None),
        Reading::Value(_) => (None, None, Some("not a finite number".to_string())),
        Reading::Text(t) => (None, Some(t), None),
        Reading::Unavailable(why) => (None, None, Some(why)),
    };
    SensorDto {
        key: info.key.as_str(),
        category: info.category.slug(),
        label: &info.label,
        quantity: info.quantity.slug(),
        source: &info.source,
        value,
        text,
        unavailable,
    }
}

/// The catalog grouped by category in display order, catalog order within.
fn grouped(catalog: &[SensorInfo]) -> Vec<(&'static str, Vec<&SensorInfo>)> {
    Category::ALL
        .iter()
        .map(|category| {
            let members: Vec<&SensorInfo> =
                catalog.iter().filter(|i| i.category == *category).collect();
            (title(*category), members)
        })
        .filter(|(_, members)| !members.is_empty())
        .collect()
}

fn json(
    catalog: &[SensorInfo],
    snapshot: &Snapshot,
    took: Option<Duration>,
    pretty: bool,
) -> serde_json::Result<String> {
    let sensors = grouped(catalog)
        .into_iter()
        .flat_map(|(_, members)| members)
        .map(|info| sensor_dto(info, snapshot.get(&info.key)))
        .collect();
    let dto = SampleDto {
        sample_millis: took.map(|t| u64::try_from(t.as_millis()).unwrap_or(u64::MAX)),
        sensors,
    };
    let mut text = if pretty {
        serde_json::to_string_pretty(&dto)?
    } else {
        serde_json::to_string(&dto)?
    };
    text.push('\n');
    Ok(text)
}

fn table(catalog: &[SensorInfo], snapshot: &Snapshot, took: Option<Duration>) -> String {
    let width = catalog
        .iter()
        .map(|i| i.label.chars().count())
        .max()
        .unwrap_or(0)
        .min(44);
    let mut out = String::new();
    for (title, members) in grouped(catalog) {
        out.push_str(title);
        out.push('\n');
        for info in members {
            let reading = snapshot.get(&info.key);
            let value = format_reading(&reading, info.quantity, DisplayFormat::default());
            let line = format!("  {:<width$}  {value:>12}  {}", info.label, info.key);
            out.push_str(line.trim_end());
            if let Reading::Unavailable(why) = &reading {
                out.push_str(&format!("  ({why})"));
            }
            out.push('\n');
        }
    }
    if catalog.is_empty() {
        out.push_str("No sensor found.\n");
    }
    if let Some(took) = took {
        out.push_str(&format!("sample took {} ms\n", took.as_millis()));
    }
    out
}

fn render(
    args: &SensorsArgs,
    catalog: &[SensorInfo],
    snapshot: &Snapshot,
    took: Duration,
) -> anyhow::Result<String> {
    let took = args.timing.then_some(took);
    if args.json {
        Ok(json(catalog, snapshot, took, args.watch.is_none())?)
    } else {
        Ok(table(catalog, snapshot, took))
    }
}

/// Parses `--watch` seconds (fractions allowed, at least the core's
/// [`MIN_REFRESH_SECONDS`]).
pub fn parse_interval(text: &str) -> Result<Duration, String> {
    let secs: f64 = text
        .parse()
        .map_err(|_| format!("`{text}` is not a number of seconds"))?;
    let fastest = f64::from(MIN_REFRESH_SECONDS);
    if !secs.is_finite() || secs < fastest {
        return Err(format!("the interval must be at least {fastest} s"));
    }
    Ok(Duration::from_secs_f64(secs))
}

/// Writes `text`; `Ok(false)` when the reader went away (`| head`).
fn emit(out: &mut dyn Write, text: &str) -> io::Result<bool> {
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(false),
        Err(e) => Err(e),
    }
}

/// `bezel sensors`: samples `source` and writes the result to `out`, once
/// or every `--watch` interval (forever, or `--count` times).
pub fn run(
    args: &SensorsArgs,
    source: &mut dyn SensorSource,
    out: &mut dyn Write,
    style: WatchStyle,
) -> anyhow::Result<()> {
    let catalog = source.catalog()?;
    source.sample()?;
    std::thread::sleep(WARM_UP);
    let mut printed = 0u64;
    loop {
        let started = Instant::now();
        let snapshot = source.sample()?;
        let took = started.elapsed();
        let mut text = render(args, &catalog, &snapshot, took)?;
        if args.watch.is_some() && style == WatchStyle::Redraw && !args.json {
            text.insert_str(0, "\x1b[2J\x1b[H");
        }
        if !emit(out, &text)? {
            return Ok(());
        }
        printed += 1;
        let Some(interval) = args.watch else {
            return Ok(());
        };
        if args.count.is_some_and(|n| printed >= n) {
            return Ok(());
        }
        std::thread::sleep(interval.saturating_sub(started.elapsed()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::sensor::Quantity;
    use bezel_sensors::FakeSensors;

    fn args(json: bool, watch: Option<f64>, count: Option<u64>, timing: bool) -> SensorsArgs {
        SensorsArgs {
            json,
            watch: watch.map(Duration::from_secs_f64),
            count,
            timing,
        }
    }

    fn run_fake(a: &SensorsArgs, style: WatchStyle) -> String {
        let mut out = Vec::new();
        run(a, &mut FakeSensors::demo(), &mut out, style).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn table_is_grouped_and_formatted() {
        let text = run_fake(&args(false, None, None, true), WatchStyle::Append);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "CPU");
        assert!(
            text.contains("\nGPU\n") && text.contains("\nSystem\n"),
            "{text}"
        );
        let usage = lines.iter().find(|l| l.ends_with("cpu.usage")).unwrap();
        let expected = format_reading(
            &Reading::Value(12.5),
            Quantity::Percent,
            DisplayFormat::default(),
        );
        assert!(usage.contains(&expected), "{usage}");
        let power = lines.iter().find(|l| l.contains("cpu.power")).unwrap();
        assert!(
            power.contains("—") && power.contains("(demo: the RAPL"),
            "{power}"
        );
        assert!(text.contains("1.5 MiB/s"), "{text}");
        assert!(text.contains("1d 02:03"), "{text}");
        assert!(lines.last().unwrap().starts_with("sample took "), "{text}");
    }

    #[test]
    fn json_lists_every_sensor_once() {
        let text = run_fake(&args(true, None, None, false), WatchStyle::Append);
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(json.get("sampleMillis").is_none());
        let sensors = json["sensors"].as_array().unwrap();
        assert_eq!(sensors.len(), FakeSensors::demo().catalog().unwrap().len());
        let usage = sensors.iter().find(|s| s["key"] == "cpu.usage").unwrap();
        assert_eq!(usage["value"], 12.5);
        assert_eq!(usage["quantity"], "percent");
        assert!(usage.get("text").is_none() && usage.get("unavailable").is_none());
        let rate = sensors.iter().find(|s| s["key"] == "net.down").unwrap();
        assert_eq!(rate["quantity"], "bytesPerSecond");
        assert_eq!(rate["category"], "network");
    }

    #[test]
    fn watch_repeats_and_redraws_only_tables() {
        let text = run_fake(&args(true, Some(0.25), Some(2), true), WatchStyle::Redraw);
        let docs: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(docs.len(), 2);
        assert!(docs.iter().all(|d| d["sampleMillis"].is_u64()));
        let text = run_fake(&args(false, Some(0.25), Some(2), false), WatchStyle::Redraw);
        assert_eq!(text.matches("\x1b[2J").count(), 2);
    }

    #[test]
    fn empty_catalogs_non_finite_values_and_closed_pipes() {
        let a = args(false, None, None, false);
        let mut out = Vec::new();
        run(
            &a,
            &mut FakeSensors::default(),
            &mut out,
            WatchStyle::Append,
        )
        .unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "No sensor found.\n");
        let catalog = FakeSensors::demo().catalog().unwrap();
        let dto = sensor_dto(&catalog[0], Reading::Value(f64::NAN));
        assert_eq!(dto.unavailable.as_deref(), Some("not a finite number"));
        struct Closed;
        impl Write for Closed {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let watch = args(false, Some(0.25), None, false);
        run(
            &watch,
            &mut FakeSensors::demo(),
            &mut Closed,
            WatchStyle::Append,
        )
        .unwrap();
        assert_eq!(Category::Board.slug(), "board");
        assert_eq!(Quantity::Amperes.slug(), "amperes");
    }

    #[test]
    fn intervals_parse() {
        assert_eq!(parse_interval("0.5"), Ok(Duration::from_millis(500)));
        assert_eq!(parse_interval("2"), Ok(Duration::from_secs(2)));
        assert!(parse_interval("0.1").is_err());
        assert!(parse_interval("inf").is_err());
        assert!(parse_interval("soon").is_err());
    }
}
