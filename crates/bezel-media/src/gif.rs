//! GIF timing read natively: how many pictures a GIF holds and how long each
//! one is shown, from its blocks, without decoding a pixel. An animated GIF
//! is a moving picture: a theme can use it as a video background, and a
//! screen gets it converted to a video.
//!
//! Delays follow ffmpeg's GIF reader, which converts and decodes these files:
//! a picture keeps the delay of the last Graphic Control Extension before it
//! (10 hundredths of a second before any), and a delay under 2 hundredths
//! plays as 10 (browsers do the same).

use std::io::{self, Read};

/// Delay of a picture with none, hundredths of a second (ffmpeg's
/// `default_delay`).
const DEFAULT_DELAY_CS: u32 = 10;
/// Shortest delay played as it is, hundredths of a second (ffmpeg's
/// `min_delay`).
const MIN_DELAY_CS: u32 = 2;

/// Block introducers and labels (GIF89a).
const EXTENSION: u8 = 0x21;
const IMAGE: u8 = 0x2C;
const TRAILER: u8 = 0x3B;
const GRAPHIC_CONTROL: u8 = 0xF9;

/// The pictures of a GIF and how long they play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Timing {
    /// Pictures in one pass.
    pub(crate) frames: u32,
    /// One pass, hundredths of a second.
    pub(crate) total_cs: u64,
    /// The shortest delay of a picture, hundredths of a second.
    pub(crate) shortest_cs: u32,
}

/// The timing of the GIF `reader` reads, or `None` when it is not a GIF. A
/// file cut short counts the pictures before the cut.
pub(crate) fn timing(reader: impl Read) -> Option<Timing> {
    let mut reader = Blocks(reader);
    let header = reader.bytes::<6>().ok()?;
    if &header[..3] != b"GIF" {
        return None;
    }
    let screen = reader.bytes::<7>().ok()?;
    reader.skip_table(screen[4]).ok()?;
    let mut timing = Timing {
        frames: 0,
        total_cs: 0,
        shortest_cs: u32::MAX,
    };
    let mut delay = DEFAULT_DELAY_CS;
    while let Ok([block]) = reader.bytes::<1>() {
        let read = match block {
            EXTENSION => reader.extension().map(|found| {
                if let Some(cs) = found {
                    delay = if cs < MIN_DELAY_CS {
                        DEFAULT_DELAY_CS
                    } else {
                        cs
                    };
                }
            }),
            IMAGE => reader.image().map(|()| {
                timing.frames += 1;
                timing.total_cs += u64::from(delay);
                timing.shortest_cs = timing.shortest_cs.min(delay);
            }),
            TRAILER => break,
            // A block no GIF has: the pictures end here.
            _ => break,
        };
        if read.is_err() {
            break;
        }
    }
    // A GIF without a picture is not one.
    (timing.frames > 0).then_some(timing)
}

/// The blocks of a GIF stream.
struct Blocks<R>(R);

impl<R: Read> Blocks<R> {
    fn bytes<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut out = [0u8; N];
        self.0.read_exact(&mut out)?;
        Ok(out)
    }

    fn skip(&mut self, n: u64) -> io::Result<()> {
        let skipped = io::copy(&mut (&mut self.0).take(n), &mut io::sink())?;
        if skipped < n {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(())
    }

    /// Skips the color table a packed field announces.
    fn skip_table(&mut self, packed: u8) -> io::Result<()> {
        if packed & 0x80 == 0 {
            return Ok(());
        }
        self.skip(3 << ((packed & 0x07) + 1))
    }

    /// Skips data sub-blocks up to their terminator; the first one is
    /// returned (up to 4 bytes).
    fn sub_blocks(&mut self) -> io::Result<Vec<u8>> {
        let mut first = None;
        loop {
            let [len] = self.bytes::<1>()?;
            if len == 0 {
                return Ok(first.unwrap_or_default());
            }
            if first.is_none() {
                let mut data = vec![0u8; usize::from(len)];
                self.0.read_exact(&mut data)?;
                first = Some(data);
            } else {
                self.skip(u64::from(len))?;
            }
        }
    }

    /// An extension: the delay of a Graphic Control Extension, else `None`.
    fn extension(&mut self) -> io::Result<Option<u32>> {
        let [label] = self.bytes::<1>()?;
        let data = self.sub_blocks()?;
        if label != GRAPHIC_CONTROL || data.len() < 3 {
            return Ok(None);
        }
        Ok(Some(u32::from(u16::from_le_bytes([data[1], data[2]]))))
    }

    /// An image: its descriptor, local color table and data.
    fn image(&mut self) -> io::Result<()> {
        let descriptor = self.bytes::<9>()?;
        self.skip_table(descriptor[8])?;
        let _min_code_size = self.bytes::<1>()?;
        self.sub_blocks().map(drop)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A GIF of 1x1 pictures shown for `delays` hundredths of a second each
    /// (`None`: no Graphic Control Extension before that picture), with a
    /// global color table and a comment.
    pub(crate) fn gif(delays: &[Option<u16>]) -> Vec<u8> {
        let mut out = b"GIF89a".to_vec();
        out.extend([1, 0, 1, 0, 0x80, 0, 0]);
        out.extend([0, 0, 0, 255, 255, 255]);
        out.extend([EXTENSION, 0xFE, 3, b'h', b'i', b'!', 0]);
        for delay in delays {
            if let Some(cs) = delay {
                let [lo, hi] = cs.to_le_bytes();
                out.extend([EXTENSION, GRAPHIC_CONTROL, 4, 0, lo, hi, 0, 0]);
            }
            out.extend([IMAGE, 0, 0, 0, 0, 1, 0, 1, 0, 0]);
            out.extend([2, 2, 0x4C, 0x01, 0]);
        }
        out.push(TRAILER);
        out
    }

    #[test]
    fn delays_add_up_like_ffmpeg_plays_them() {
        let bytes = gif(&[Some(10), Some(7), Some(0), None, Some(300)]);
        let timing = timing(bytes.as_slice()).unwrap();
        // 0 plays as 10; the picture without its own delay keeps the last.
        assert_eq!(
            timing,
            Timing {
                frames: 5,
                total_cs: 10 + 7 + 10 + 10 + 300,
                shortest_cs: 7
            }
        );
        let first = timing_of(&[None]);
        assert_eq!((first.frames, first.total_cs), (1, 10), "the default");
    }

    fn timing_of(delays: &[Option<u16>]) -> Timing {
        timing(gif(delays).as_slice()).unwrap()
    }

    #[test]
    fn a_cut_file_counts_what_came_before_and_garbage_is_no_gif() {
        let mut bytes = gif(&[Some(5), Some(5), Some(5)]);
        bytes.truncate(bytes.len() - 8);
        assert_eq!(timing(bytes.as_slice()).unwrap().frames, 2);
        assert_eq!(timing(&b"PNG and more bytes"[..]), None);
        assert_eq!(timing(&b"GIF89a"[..]), None);
        let empty = [b"GIF89a".as_slice(), &[1, 0, 1, 0, 0, 0, 0, TRAILER]].concat();
        assert_eq!(timing(empty.as_slice()), None, "no picture");
    }

    #[test]
    fn gifs_written_by_an_encoder_read_the_same() {
        use image::codecs::gif::{GifEncoder, Repeat};
        use image::{Delay, Frame, Rgba, RgbaImage};

        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            encoder.set_repeat(Repeat::Infinite).unwrap();
            let frames = [(100, 10), (70, 20), (300, 30)].map(|(ms, red)| {
                let picture = RgbaImage::from_pixel(4, 3, Rgba([red, 0, 0, 255]));
                Frame::from_parts(picture, 0, 0, Delay::from_numer_denom_ms(ms, 1))
            });
            encoder.encode_frames(frames).unwrap();
        }
        let timing = timing(bytes.as_slice()).unwrap();
        assert_eq!((timing.frames, timing.total_cs), (3, 47));
        assert_eq!(timing.shortest_cs, 7);
    }
}
