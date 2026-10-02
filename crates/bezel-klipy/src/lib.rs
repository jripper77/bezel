//! Bezel KLIPY: the driven adapter behind the core's
//! [`GifSource`](bezel_core::ports::GifSource) port, on KLIPY's API with the
//! user's own key (D-2026-10-01-gif-sticker-search-2).
//!
//! [`KlipyClient`] asks `https://api.klipy.com/api/v1/{key}/{gifs|stickers}/
//! {search|trending}` for a page of GIFs or stickers and reads their files
//! from `https://static.klipy.com`:
//! - HTTPS only, with rustls (ring and the webpki roots), a 10 s timeout and
//!   no redirect followed: the key is in the API path, so an answer that
//!   points elsewhere is an error, never a second request;
//! - the key goes in the API path and nowhere else: files are read without
//!   it, only from KLIPY's file host, up to the byte limit the core gives;
//! - the query names the page, 24 items, the customer id made once per key
//!   ([`new_customer_id`]), the locale (`BR` in Portuguese, none in English),
//!   the content filter (`medium`, or `off` when explicit results are
//!   shown) and the formats (`gif,jpg`);
//! - answers are read tolerantly: unknown fields are ignored, renditions it
//!   cannot read are skipped and an item without a GIF is dropped;
//! - HTTP 429 is [`ServiceFailure::RateLimited`], 401 and 403
//!   [`ServiceFailure::KeyRejected`], anything else
//!   [`ServiceFailure::Unavailable`] with a fixed text: no error, message or
//!   `Debug` output carries the key or an address.
//!
//! The adapter decides nothing: which rendition is collected or previewed,
//! and what the switch for explicit results means, are the core's.
//!
//! [`ServiceFailure::RateLimited`]: bezel_core::domain::error::ServiceFailure::RateLimited
//! [`ServiceFailure::KeyRejected`]: bezel_core::domain::error::ServiceFailure::KeyRejected
//! [`ServiceFailure::Unavailable`]: bezel_core::domain::error::ServiceFailure::Unavailable
#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod client;
mod dto;

pub use client::{KlipyClient, new_customer_id};
