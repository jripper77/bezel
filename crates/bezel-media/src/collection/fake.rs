//! A [`GifSource`] that answers scripted pages and files: the fake provider
//! behind the tests of the core's GIF use cases and of the studio's
//! commands. It never reaches a network.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use bezel_core::domain::error::ServiceFailure;
use bezel_core::domain::gifs::{GifKind, GifPage, GifQuery, Rendition};
use bezel_core::ports::GifSource;
use bezel_core::{BezelError, Result};

/// A request the source received, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GifCall {
    /// [`GifSource::page`] for this query.
    Page(GifQuery),
    /// [`GifSource::download`] of the rendition at `location`, read up to
    /// `limit` bytes.
    Download {
        /// The rendition's location.
        location: String,
        /// The byte limit asked for.
        limit: u64,
    },
}

/// Answers the pages and files it was given: a page by kind, text and page
/// number (an empty last page for any other query), a file by location (an
/// unavailable service for any other). Clones share the record of calls,
/// so a test keeps one to see what was asked.
#[derive(Debug, Clone, Default)]
pub struct FakeGifSource {
    pages: BTreeMap<(GifKind, String, u32), GifPage>,
    files: BTreeMap<String, Vec<u8>>,
    failure: Option<BezelError>,
    calls: Arc<Mutex<Vec<GifCall>>>,
}

impl FakeGifSource {
    /// The provider name it records as the origin of collected items.
    pub const PROVIDER: &'static str = "fake";

    /// A source with no page and no file.
    pub fn new() -> Self {
        Self::default()
    }

    /// Answers `answer` to the page `page` of the search for `text`
    /// (trimmed; empty for the trending items) among `kind`.
    #[must_use]
    pub fn with_page(mut self, kind: GifKind, text: &str, page: u32, answer: GifPage) -> Self {
        self.pages
            .insert((kind, text.trim().to_string(), page), answer);
        self
    }

    /// Serves `bytes` for the rendition at `location`.
    #[must_use]
    pub fn with_file(mut self, location: &str, bytes: Vec<u8>) -> Self {
        self.files.insert(location.to_string(), bytes);
        self
    }

    /// Fails every request with `error` (recorded all the same).
    #[must_use]
    pub fn failing(mut self, error: BezelError) -> Self {
        self.failure = Some(error);
        self
    }

    /// Every request received, in order.
    pub fn calls(&self) -> Vec<GifCall> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The locations downloaded, with the limit each was read with.
    pub fn downloads(&self) -> Vec<(String, u64)> {
        let downloads = self.calls().into_iter().filter_map(|call| match call {
            GifCall::Download { location, limit } => Some((location, limit)),
            GifCall::Page(_) => None,
        });
        downloads.collect()
    }

    fn record(&self, call: GifCall) -> Result<()> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
        self.failure.clone().map_or(Ok(()), Err)
    }
}

impl GifSource for FakeGifSource {
    fn provider(&self) -> &str {
        Self::PROVIDER
    }

    fn page(&self, query: &GifQuery) -> Result<GifPage> {
        self.record(GifCall::Page(query.clone()))?;
        let key = (query.kind, query.text.trim().to_string(), query.page);
        Ok(self.pages.get(&key).cloned().unwrap_or_default())
    }

    fn download(&self, rendition: &Rendition, limit: u64) -> Result<Vec<u8>> {
        let location = rendition.location.clone();
        self.record(GifCall::Download {
            location: location.clone(),
            limit,
        })?;
        let Some(bytes) = self.files.get(&location) else {
            let missing = format!("no file at {location}");
            return Err(BezelError::Service(ServiceFailure::Unavailable(missing)));
        };
        if bytes.len() as u64 > limit {
            return Err(BezelError::InvalidInput(format!(
                "the file is larger than {limit} bytes"
            )));
        }
        Ok(bytes.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bezel_core::domain::clock::Language;
    use bezel_core::domain::gifs::{Explicit, GifItem, RenditionFormat, Tier};

    fn rendition(location: &str) -> Rendition {
        Rendition {
            tier: Tier::Small,
            format: RenditionFormat::Gif,
            location: location.into(),
            width: 1,
            height: 1,
            bytes: None,
        }
    }

    #[test]
    fn answers_what_it_was_given_and_records_every_request() {
        let item = GifItem {
            id: "7".into(),
            title: "cat".into(),
            kind: GifKind::Sticker,
            page_url: None,
            renditions: vec![rendition("files/7.gif")],
        };
        let page = GifPage {
            items: vec![item],
            has_next: true,
        };
        let source = FakeGifSource::new()
            .with_page(GifKind::Sticker, " cat ", 1, page.clone())
            .with_file("files/7.gif", b"GIF89a".to_vec());
        let watcher = source.clone();
        assert_eq!(source.provider(), "fake");

        let query = GifQuery::new(
            GifKind::Sticker,
            "cat",
            1,
            Explicit::Hidden,
            Language::English,
        );
        assert_eq!(source.page(&query).unwrap(), page);
        assert_eq!(source.page(&query.next()).unwrap(), GifPage::default());
        assert_eq!(
            source.download(&rendition("files/7.gif"), 6).unwrap(),
            b"GIF89a"
        );
        assert!(matches!(
            source.download(&rendition("files/7.gif"), 5),
            Err(BezelError::InvalidInput(_))
        ));
        assert!(matches!(
            source.download(&rendition("files/8.gif"), 9),
            Err(BezelError::Service(ServiceFailure::Unavailable(_)))
        ));
        assert_eq!(watcher.calls().len(), 5);
        assert_eq!(watcher.calls()[0], GifCall::Page(query));
        assert_eq!(
            watcher.downloads(),
            [
                ("files/7.gif".to_string(), 6),
                ("files/7.gif".to_string(), 5),
                ("files/8.gif".to_string(), 9)
            ]
        );

        let limited =
            FakeGifSource::new().failing(BezelError::Service(ServiceFailure::RateLimited));
        let trending = GifQuery::new(GifKind::Gif, "", 1, Explicit::Shown, Language::English);
        assert_eq!(
            limited.page(&trending),
            Err(BezelError::Service(ServiceFailure::RateLimited))
        );
        assert_eq!(limited.calls(), [GifCall::Page(trending)]);
    }
}
