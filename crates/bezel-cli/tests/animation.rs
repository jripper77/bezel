//! An animated GIF element on a rev C screen (T-7.11): the core's runtime
//! draws a frame at the GIF's next frame time, the real renderer moves only
//! the GIF, and the rev C driver sends that frame as a partial update
//! carrying exactly the GIF's rectangle, never a full frame. The screen is
//! the driver over a scripted wire: no hardware.
#![allow(clippy::expect_used, clippy::panic)] // helpers of a failing test panic

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use bezel_core::app::ThemeRuntime;
use bezel_core::domain::catalog::model_by_id;
use bezel_core::domain::clock::{Language, LocalTime};
use bezel_core::domain::device::ModelId;
use bezel_core::domain::geometry::{Orientation, Size};
use bezel_core::domain::theme::{AssetRef, BoxF, Element, ElementId, ElementKind, Fit, Theme};
use bezel_core::ports::ScreenLink;
use bezel_devices::driver::Pause;
use bezel_devices::driver::turing_rev_c::TuringRevC;
use bezel_devices::protocol::turing_rev_c::{BLOCK, BLOCK_PAYLOAD, op};
use bezel_devices::wire::ScriptedWire;
use bezel_render::{SkiaRenderer, SystemFonts};
use bezel_sensors::FakeSensors;
use image::codecs::gif::{GifEncoder, Repeat};

const TIME: LocalTime = LocalTime {
    year: 2026,
    month: 9,
    day: 30,
    hour: 21,
    minute: 5,
    second: 0,
    weekday: 2,
};

#[derive(Clone)]
struct NoPause;

impl Pause for NoPause {
    fn pause(&self, _: Duration) {}
}

/// A GIF of four 100 ms frames of solid colors.
fn gif() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut out);
        encoder.set_repeat(Repeat::Infinite).expect("repeat");
        for (r, b) in [(255, 0), (0, 255), (255, 255), (0, 0)] {
            let pixels = image::RgbaImage::from_pixel(8, 8, image::Rgba([r, 90, b, 255]));
            let delay = image::Delay::from_numer_denom_ms(100, 1);
            encoder
                .encode_frame(image::Frame::from_parts(pixels, 0, 0, delay))
                .expect("frame");
        }
    }
    out
}

/// A horizontal 8.8" theme with a 64x64 GIF element at (100, 100).
fn theme() -> (Theme, BTreeMap<AssetRef, Vec<u8>>) {
    let mut theme = Theme::blank("gif", Size::new(480, 1920), Orientation::Landscape);
    let asset = AssetRef("assets/spin.gif".into());
    theme.elements.push(Element {
        card: None,
        card_member: None,
        id: ElementId(1),
        name: "spin".into(),
        frame: BoxF::new(100.0, 100.0, 64.0, 64.0),
        opacity: 1.0,
        visible: true,
        locked: false,
        kind: ElementKind::Image {
            asset: asset.clone(),
            fit: Fit::Fill,
        },
    });
    (theme, BTreeMap::from([(asset, gif())]))
}

/// The pixel indices a raw-BGRA run list (spec § 9.2) writes.
fn written(list: &[u8]) -> Vec<usize> {
    let mut pixels = Vec::new();
    let mut i = 0;
    while i + 3 <= list.len() {
        let head =
            usize::from(list[i]) << 16 | usize::from(list[i + 1]) << 8 | usize::from(list[i + 2]);
        if head & 0x80_0000 != 0 {
            pixels.push(head & 0x7F_FFFF);
            i += 3 + 4;
        } else {
            let count = usize::from(list[i + 3]) << 8 | usize::from(list[i + 4]);
            pixels.extend(head..head + count);
            i += 5 + count * 4;
        }
    }
    pixels
}

#[test]
fn an_animation_frame_sends_only_the_gifs_rectangle() {
    let model = model_by_id(ModelId("turing-8.8")).expect("8.8");
    let wire =
        ScriptedWire::with_replies([b"chs_88inch.dev1_rom1.90".to_vec(), b"media_stop".to_vec()]);
    let mut screen = TuringRevC::connect(wire, &NoPause, &[model]).expect("connects");
    screen
        .set_orientation(Orientation::Landscape)
        .expect("turns");
    let (theme, assets) = theme();
    let mut runtime = ThemeRuntime::new(theme, assets, Language::English);
    let mut renderer = SkiaRenderer::with_fonts(Vec::new(), SystemFonts::Skip);
    let mut sensors = FakeSensors::demo();

    let first = runtime
        .live_frame(&mut sensors, &mut renderer, TIME, Duration::ZERO)
        .expect("frame");
    screen.present(&first).expect("full frame");
    assert_eq!(runtime.next_due(), Duration::from_millis(100), "the GIF's");
    let before = screen.wire().sent.len();

    let next = runtime
        .live_frame(&mut sensors, &mut renderer, TIME, runtime.next_due())
        .expect("frame");
    assert_eq!(
        sensors.samples_taken(),
        1,
        "no sample for an animation frame"
    );
    screen.present(&next).expect("partial");
    let sent = &screen.wire().sent[before..];
    let opcodes: BTreeSet<u8> = sent
        .iter()
        .filter(|p| p.len() == BLOCK && p[1..3] == [0xEF, 0x69])
        .map(|p| p[0])
        .collect();
    assert!(opcodes.contains(&op::UPDATE_BITMAP), "{opcodes:x?}");
    assert!(!opcodes.contains(&op::DISPLAY_BITMAP), "no full frame");

    // The run list: header bytes 3..7 give its length, the next write
    // carries it in 249-byte blocks.
    let header = &sent[0];
    assert_eq!(header[0], op::UPDATE_BITMAP);
    let len = u32::from_be_bytes([header[3], header[4], header[5], header[6]]) as usize;
    let list: Vec<u8> = sent[1]
        .chunks(BLOCK)
        .flat_map(|block| &block[..BLOCK_PAYLOAD])
        .copied()
        .take(len)
        .collect();
    assert_eq!(&list[len - 2..], &[0xEF, 0x69]);
    let pixels = written(&list[..len - 2]);
    assert_eq!(pixels.len(), 64 * 64, "every pixel of the GIF, no other");
    let native = model.panel.width as usize;
    let (xs, ys): (Vec<usize>, Vec<usize>) =
        pixels.iter().map(|p| (p % native, p / native)).unzip();
    let span = |v: &[usize]| v.iter().max().expect("x") - v.iter().min().expect("x") + 1;
    assert_eq!((span(&xs), span(&ys)), (64, 64), "one 64x64 rectangle");
    assert_eq!(
        len,
        64 * (5 + 64 * 4) + 2,
        "64 runs of 64 pixels, not 3.7 MB"
    );
}
