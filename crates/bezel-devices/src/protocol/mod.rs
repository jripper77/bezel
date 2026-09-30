//! Wire formats of each screen family. Pure byte builders and parsers; the
//! drivers in `crate::driver` put them on a transport.

pub mod kipye_rev_d;
pub mod rgb565;
pub mod turing_rev_a;
pub mod turing_rev_c;
pub mod turing_usb;
pub mod wch;
pub mod weact;
pub mod xuanfang_rev_b;
