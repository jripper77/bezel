//! [`KlipyClient`]: KLIPY's API behind the core's [`GifSource`] port.

use std::fmt::{self, Write as _};
use std::io;
use std::time::Duration;

use bezel_core::domain::clock::Language;
use bezel_core::domain::error::ServiceFailure;
use bezel_core::domain::gifs::{ContentFilter, GifKind, GifPage, GifQuery, PAGE_SIZE, Rendition};
use bezel_core::domain::storage::MIB;
use bezel_core::ports::GifSource;
use bezel_core::{BezelError, Result};
use ureq::http::Response;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ureq::{Agent, Body};

use crate::dto;

/// KLIPY's API, version 1; the key is the first path segment after it.
const API_BASE: &str = "https://api.klipy.com/api/v1";

/// KLIPY's file host: files are read from it and from nowhere else.
const FILES_ORIGIN: &str = "https://static.klipy.com";

/// The longest a request may take, from resolving the name to the last
/// byte of the answer.
const TIMEOUT: Duration = Duration::from_secs(10);

/// Most bytes of an API answer; a page of 24 items is far smaller.
const ANSWER_LIMIT: u64 = 4 * MIB;

/// Where the client sends its requests.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Endpoints {
    /// The API's base address, before the key.
    api: String,
    /// The file host's origin: scheme and host, without a slash.
    files: String,
    /// Whether an address without HTTPS is refused.
    https_only: bool,
}

impl Endpoints {
    /// KLIPY's own hosts, over HTTPS only: the only endpoints outside the
    /// tests.
    fn production() -> Self {
        Self {
            api: API_BASE.to_string(),
            files: FILES_ORIGIN.to_string(),
            https_only: true,
        }
    }

    /// A plain-HTTP server on the loopback, standing for both hosts.
    #[cfg(test)]
    fn loopback(addr: std::net::SocketAddr) -> Self {
        Self {
            api: format!("http://{addr}/api/v1"),
            files: format!("http://{addr}"),
            https_only: false,
        }
    }
}

/// The [`GifSource`] on KLIPY's API with the user's own key
/// (D-2026-10-01-gif-sticker-search-2).
///
/// The studio makes one per saved key with [`KlipyClient::new`], giving it
/// the key and the customer id kept with it ([`new_customer_id`]); the
/// locale comes with each query, from its `language`. Making one sends
/// nothing: a request leaves only when a page or a file is asked for.
pub struct KlipyClient {
    agent: Agent,
    key: String,
    customer_id: String,
    endpoints: Endpoints,
}

impl KlipyClient {
    /// The provider's name, recorded as the origin of collected items.
    pub const PROVIDER: &'static str = "klipy";

    /// A client of `https://api.klipy.com` with the user's `key` and the
    /// `customer_id` made for that key.
    pub fn new(key: &str, customer_id: &str) -> Self {
        Self::with_endpoints(key, customer_id, Endpoints::production())
    }

    fn with_endpoints(key: &str, customer_id: &str, endpoints: Endpoints) -> Self {
        let tls = TlsConfig::builder()
            .provider(TlsProvider::Rustls)
            .root_certs(RootCerts::WebPki)
            .build();
        let agent = Agent::config_builder()
            .tls_config(tls)
            .https_only(endpoints.https_only)
            .max_redirects(0)
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .build()
            .new_agent();
        Self {
            agent,
            key: key.to_string(),
            customer_id: customer_id.to_string(),
            endpoints,
        }
    }

    /// The address of `query`'s page, the key in its path.
    fn page_address(&self, query: &GifQuery) -> String {
        let action = if query.trending() {
            "trending"
        } else {
            "search"
        };
        format!(
            "{}/{}/{}/{action}",
            self.endpoints.api,
            path_segment(&self.key),
            dto::section(query.kind)
        )
    }

    /// The query string of `query`'s page, in this order.
    fn page_params(&self, query: &GifQuery) -> Vec<(&'static str, String)> {
        let mut params = Vec::with_capacity(7);
        if !query.trending() {
            params.push(("q", query.text.clone()));
        }
        params.push(("page", query.page.to_string()));
        params.push(("per_page", PAGE_SIZE.to_string()));
        params.push(("customer_id", self.customer_id.clone()));
        if let Some(locale) = locale(query.language) {
            params.push(("locale", locale.to_string()));
        }
        params.push(("content_filter", filter_name(query.filter()).to_string()));
        params.push(("format_filter", format_filter(query.kind).to_string()));
        params
    }

    /// `location` when it is on the file host; a file anywhere else is
    /// never asked for.
    fn file_address<'a>(&self, location: &'a str) -> Result<&'a str> {
        let on_host = location
            .strip_prefix(self.endpoints.files.as_str())
            .is_some_and(|path| path.starts_with('/'));
        if on_host {
            Ok(location)
        } else {
            Err(unavailable("a file outside KLIPY's file host was not read"))
        }
    }
}

impl GifSource for KlipyClient {
    fn provider(&self) -> &str {
        Self::PROVIDER
    }

    fn page(&self, query: &GifQuery) -> Result<GifPage> {
        let mut response = self
            .agent
            .get(self.page_address(query))
            .query_pairs(self.page_params(query))
            .call()
            .map_err(|error| failure(&error))?;
        let status = response.status().as_u16();
        api_status(status)?;
        let body = read_up_to(&mut response, ANSWER_LIMIT);
        api_answer(status, body.as_deref().unwrap_or_default())?;
        let body = body.map_err(|error| failure(&error))?;
        dto::read_page(&body, query.kind, &self.endpoints.files)
            .ok_or_else(|| unavailable("the answer was not a page of results"))
    }

    fn download(&self, rendition: &Rendition, limit: u64) -> Result<Vec<u8>> {
        let address = self.file_address(&rendition.location)?;
        let mut response = self
            .agent
            .get(address)
            .call()
            .map_err(|error| failure(&error))?;
        file_status(response.status().as_u16())?;
        if response.body().content_length().is_some_and(|n| n > limit) {
            return Err(too_large(limit));
        }
        read_up_to(&mut response, limit).map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => too_large(limit),
            other => failure(&other),
        })
    }
}

/// The client without its key nor its customer id.
impl fmt::Debug for KlipyClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KlipyClient")
            .field("provider", &Self::PROVIDER)
            .finish_non_exhaustive()
    }
}

/// A new customer id for a saved key: 128 random bits as 32 lowercase
/// hexadecimal digits, never personal data
/// (D-2026-10-01-gif-sticker-search-2). The studio makes one when a key is
/// saved and keeps it with the key; a new key gets a new one.
pub fn new_customer_id() -> Result<String> {
    let mut bits = [0u8; 16];
    getrandom::fill(&mut bits).map_err(|_| {
        BezelError::Unsupported("the system gave no random numbers for the customer id".into())
    })?;
    let mut id = String::with_capacity(32);
    for byte in bits {
        let _ = write!(id, "{byte:02x}");
    }
    Ok(id)
}

/// The body of `response`, failing once it has more than `limit` bytes:
/// at most one byte past the limit is read to tell.
fn read_up_to(
    response: &mut Response<Body>,
    limit: u64,
) -> std::result::Result<Vec<u8>, ureq::Error> {
    response
        .body_mut()
        .with_config()
        .limit(limit.saturating_add(1))
        .read_to_vec()
}

/// What the API's status alone tells: 429 is the rate limit; 401, 403 and
/// 404 a refused key (KLIPY answers a key it does not know with 404: the key
/// is the only part of the address that varies). Any other status waits
/// for the body ([`api_answer`]).
fn api_status(code: u16) -> Result<()> {
    match code {
        429 => Err(BezelError::Service(ServiceFailure::RateLimited)),
        401 | 403 | 404 => Err(BezelError::Service(ServiceFailure::KeyRejected)),
        _ => Ok(()),
    }
}

/// The API's answer once its body is read: a body saying the key is
/// invalid is a refused key whatever the status, then a status other than
/// 2xx is unavailable. The body itself never reaches the error.
fn api_answer(code: u16, body: &[u8]) -> Result<()> {
    if dto::refuses_the_key(body) {
        return Err(BezelError::Service(ServiceFailure::KeyRejected));
    }
    match code {
        200..=299 => Ok(()),
        _ => Err(unexpected_status(code)),
    }
}

/// The file host's status: it never sees the key, so only its rate limit
/// is told apart.
fn file_status(code: u16) -> Result<()> {
    match code {
        200..=299 => Ok(()),
        429 => Err(BezelError::Service(ServiceFailure::RateLimited)),
        _ => Err(unexpected_status(code)),
    }
}

fn unexpected_status(code: u16) -> BezelError {
    if (300..400).contains(&code) {
        unavailable(&format!("HTTP {code}: a redirect, not followed"))
    } else {
        unavailable(&format!("HTTP {code}"))
    }
}

/// A failed request as the core sees it: a fixed text for its kind, never
/// the error's own text, which can hold the address (and the address holds
/// the key).
fn failure(error: &ureq::Error) -> BezelError {
    let detail = match error {
        ureq::Error::Timeout(_) => "no answer within 10 s",
        ureq::Error::HostNotFound => "the server's name was not found; is the computer online?",
        ureq::Error::ConnectionFailed => "could not connect to the server",
        ureq::Error::Io(io) => io_detail(io.kind()),
        ureq::Error::Tls(_)
        | ureq::Error::Rustls(_)
        | ureq::Error::Pem(_)
        | ureq::Error::TlsRequired => "the secure connection failed",
        ureq::Error::RequireHttpsOnly(_) => "an address without HTTPS was refused",
        ureq::Error::TooManyRedirects | ureq::Error::RedirectFailed => {
            "a redirect was not followed"
        }
        ureq::Error::BodyExceedsLimit(_) => "the answer was too large",
        ureq::Error::LargeResponseHeader(..) => "the answer's headers were too large",
        ureq::Error::BodyStalled => "the answer stopped arriving",
        _ => "the request failed",
    };
    unavailable(detail)
}

fn io_detail(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::ConnectionRefused => "the server refused the connection",
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => "no answer within 10 s",
        io::ErrorKind::ConnectionReset
        | io::ErrorKind::ConnectionAborted
        | io::ErrorKind::BrokenPipe
        | io::ErrorKind::UnexpectedEof => "the connection closed before the answer ended",
        _ => "a network error",
    }
}

fn unavailable(detail: &str) -> BezelError {
    BezelError::Service(ServiceFailure::Unavailable(detail.to_string()))
}

fn too_large(limit: u64) -> BezelError {
    BezelError::InvalidInput(format!("the file is larger than {limit} bytes"))
}

/// KLIPY's locale for `language`: the country, Brazil, in Portuguese; none
/// in English.
const fn locale(language: Language) -> Option<&'static str> {
    match language {
        Language::PortugueseBr => Some("BR"),
        Language::English => None,
    }
}

/// The formats asked for `kind`: its GIFs and its stills for reduced
/// motion, JPEG for a GIF and PNG for a sticker. KLIPY keeps only the items
/// offered in every format named (stickers asked for `gif,jpg` come back
/// as an empty page), so each kind names its own.
const fn format_filter(kind: GifKind) -> &'static str {
    match kind {
        GifKind::Gif => "gif,jpg",
        GifKind::Sticker => "gif,png",
    }
}

/// KLIPY's name for `filter`.
const fn filter_name(filter: ContentFilter) -> &'static str {
    match filter {
        ContentFilter::Medium => "medium",
        ContentFilter::Off => "off",
    }
}

/// `text` as one path segment: letters, digits, `-` and `_` as they are,
/// any other byte percent-encoded, so a key can never add a path, a query
/// or a host.
fn path_segment(text: &str) -> String {
    let mut segment = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_' {
            segment.push(char::from(byte));
        } else {
            let _ = write!(segment, "%{byte:02X}");
        }
    }
    segment
}

#[cfg(test)]
mod tests {
    //! The real client against a std HTTP server on 127.0.0.1 that serves
    //! the recorded KLIPY answers and records every request it gets
    //! (D-2026-10-01-gif-sticker-search-6). The key is an obvious fake.

    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::thread;

    use bezel_core::app;
    use bezel_core::domain::gifs::{
        Explicit, GifItem, Motion, PREVIEW_LIMIT, RenditionFormat, Tier, is_gif,
    };

    use super::*;

    const KEY: &str = "test-key";
    const CUSTOMER: &str = "test-customer";
    /// KLIPY's real answers of 2026-10-01, cut to their first items.
    const GIFS: &str = include_str!("../fixtures/gifs-search.json");
    const STICKERS: &str = include_str!("../fixtures/stickers-trending.json");
    /// KLIPY's real answer to a key it does not know, sent with HTTP 404.
    const INVALID_KEY: &str = include_str!("../fixtures/invalid-key-404.json");

    /// A one-pixel GIF89a.
    const PIXEL: &[u8] = &[
        0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xFF, 0xFF,
        0xFF, 0x00, 0x00, 0x00, 0x21, 0xF9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2C, 0x00, 0x00,
        0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3B,
    ];

    /// The start of a PNG: its signature and the first chunk's head.
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    /// One answer of the loopback server.
    struct Reply {
        status: u16,
        headers: Vec<String>,
        body: Vec<u8>,
        sized: bool,
    }

    impl Reply {
        fn status(status: u16) -> Self {
            Self {
                status,
                headers: Vec::new(),
                body: Vec::new(),
                sized: true,
            }
        }

        fn json(text: &str) -> Self {
            Self::status(200)
                .header("Content-Type: application/json")
                .body(text.as_bytes())
        }

        fn file(bytes: &[u8]) -> Self {
            Self::status(200)
                .header("Content-Type: image/gif")
                .body(bytes)
        }

        fn header(mut self, line: &str) -> Self {
            self.headers.push(line.to_string());
            self
        }

        fn body(mut self, bytes: &[u8]) -> Self {
            self.body = bytes.to_vec();
            self
        }

        /// Without a `Content-Length`: the body ends when the connection
        /// closes.
        fn streamed(mut self) -> Self {
            self.sized = false;
            self
        }

        fn write(&self, stream: &mut TcpStream) {
            let mut head = format!("HTTP/1.1 {} Answer\r\nConnection: close\r\n", self.status);
            if self.sized {
                head.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
            }
            for line in &self.headers {
                head.push_str(line);
                head.push_str("\r\n");
            }
            head.push_str("\r\n");
            // The client may hang up early (a body over its limit).
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&self.body);
        }
    }

    /// A std HTTP/1.1 server on 127.0.0.1: one connection per reply, in
    /// order, each request's head recorded before it is answered.
    struct Loopback {
        addr: SocketAddr,
        heads: Arc<Mutex<Vec<String>>>,
    }

    impl Loopback {
        /// Serves the replies `script` makes from the server's origin (to
        /// point the recorded files at it).
        fn serve(script: impl FnOnce(&str) -> Vec<Reply>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
            let addr = listener.local_addr().expect("its address");
            let replies = script(&format!("http://{addr}"));
            let heads = Arc::new(Mutex::new(Vec::new()));
            let record = Arc::clone(&heads);
            thread::spawn(move || {
                for reply in replies {
                    let Ok((mut stream, _)) = listener.accept() else {
                        return;
                    };
                    let head = read_head(&mut stream);
                    record.lock().expect("the record").push(head);
                    reply.write(&mut stream);
                }
            });
            Self { addr, heads }
        }

        fn origin(&self) -> String {
            format!("http://{}", self.addr)
        }

        /// The real client, its endpoints on this server.
        fn client(&self) -> KlipyClient {
            KlipyClient::with_endpoints(KEY, CUSTOMER, Endpoints::loopback(self.addr))
        }

        fn heads(&self) -> Vec<String> {
            self.heads.lock().expect("the record").clone()
        }
    }

    fn read_head(stream: &mut TcpStream) -> String {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => head.push(byte[0]),
                _ => break,
            }
        }
        String::from_utf8_lossy(&head).into_owned()
    }

    /// A host that never answers: a listener nobody accepts on, to tell
    /// whether anything connected to it.
    struct Elsewhere(TcpListener);

    impl Elsewhere {
        fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
            listener.set_nonblocking(true).expect("non-blocking");
            Self(listener)
        }

        fn addr(&self) -> SocketAddr {
            self.0.local_addr().expect("its address")
        }

        fn untouched(&self) -> bool {
            matches!(self.0.accept(), Err(e) if e.kind() == io::ErrorKind::WouldBlock)
        }
    }

    /// The path of a request's head and its decoded query, in order.
    fn target(head: &str) -> (String, Vec<(String, String)>) {
        let line = head.lines().next().unwrap_or_default();
        let target = line.split(' ').nth(1).unwrap_or_default();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let pairs = query
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                (decode(name), decode(value))
            })
            .collect();
        (path.to_string(), pairs)
    }

    fn decode(text: &str) -> String {
        let mut bytes = Vec::new();
        let mut rest = text.as_bytes();
        while let Some((&byte, tail)) = rest.split_first() {
            rest = tail;
            match byte {
                b'%' if rest.len() >= 2 => {
                    let hex = std::str::from_utf8(&rest[..2]).expect("hex");
                    bytes.push(u8::from_str_radix(hex, 16).expect("a byte"));
                    rest = &rest[2..];
                }
                b'+' => bytes.push(b' '),
                other => bytes.push(other),
            }
        }
        String::from_utf8(bytes).expect("UTF-8")
    }

    fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        let pairs = expected.iter().map(|(n, v)| (n.to_string(), v.to_string()));
        pairs.collect()
    }

    fn rendition(location: &str) -> Rendition {
        Rendition {
            tier: Tier::Small,
            format: RenditionFormat::Gif,
            location: location.to_string(),
            width: 1,
            height: 1,
            bytes: None,
        }
    }

    fn search(text: &str) -> GifQuery {
        GifQuery::new(GifKind::Gif, text, 1, Explicit::Hidden, Language::English)
    }

    fn unavailable_detail(result: Result<impl fmt::Debug>) -> String {
        match result {
            Err(BezelError::Service(ServiceFailure::Unavailable(detail))) => detail,
            other => panic!("expected an unavailable service, got {other:?}"),
        }
    }

    #[test]
    fn searches_over_loopback_http() {
        let server = Loopback::serve(|origin| {
            vec![
                Reply::json(&GIFS.replace(FILES_ORIGIN, origin)),
                Reply::json(&STICKERS.replace(FILES_ORIGIN, origin)),
                Reply::file(PIXEL),
                Reply::status(200)
                    .header("Content-Type: image/png")
                    .body(PNG),
            ]
        });
        let client = server.client();
        let files = format!("{}/ii/", server.origin());

        let query = GifQuery::new(
            GifKind::Gif,
            " gato feliz ",
            1,
            Explicit::Hidden,
            Language::PortugueseBr,
        );
        let gifs = client.page(&query).expect("a page of GIFs");
        assert!(gifs.has_next);
        let titles: Vec<&str> = gifs.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Goatplaybanjo's Chatty Cat",
                "Shocked Cat's Surprised Reaction",
                "Adorable Kitten Meowing and Looking Around",
            ]
        );
        let preview = gifs.items[0].preview(Motion::Animated).expect("a preview");
        assert_eq!(
            (preview.tier, preview.format),
            (Tier::Small, RenditionFormat::Gif)
        );
        assert!(preview.location.starts_with(&files), "{}", preview.location);
        assert!(preview.location.ends_with("/M7ThMWi7.gif"));
        let still = gifs.items[0].preview(Motion::Still).map(|r| r.format);
        assert_eq!(
            still,
            Some(RenditionFormat::Jpeg),
            "a GIF's still is a JPEG"
        );

        let trending = GifQuery::new(GifKind::Sticker, "", 2, Explicit::Shown, Language::English);
        let stickers = client.page(&trending).expect("a page of stickers");
        assert!(stickers.has_next);
        assert_eq!(stickers.items.len(), 2);
        assert!(stickers.items.iter().all(|i| i.kind == GifKind::Sticker));
        let sticker = &stickers.items[0];
        let still = sticker.preview(Motion::Still).expect("a sticker's still");
        assert_eq!(
            (still.tier, still.format),
            (Tier::Small, RenditionFormat::Png)
        );
        assert!(still.location.ends_with("/ayXrZ3mBEPbcItVaiams.png"));

        let bytes = client
            .download(preview, PREVIEW_LIMIT)
            .expect("the preview");
        assert_eq!(bytes, PIXEL);
        let png = app::gifs::preview(&client, sticker, Motion::Still).expect("the still");
        assert_eq!(png.as_deref(), Some(PNG));

        let heads = server.heads();
        assert_eq!(heads.len(), 4);
        assert!(heads.iter().all(|h| h.starts_with("GET ")), "{heads:?}");
        let (path, query) = target(&heads[0]);
        assert_eq!(path, "/api/v1/test-key/gifs/search");
        let expected = [
            ("q", "gato feliz"),
            ("page", "1"),
            ("per_page", "24"),
            ("customer_id", CUSTOMER),
            ("locale", "BR"),
            ("content_filter", "medium"),
            ("format_filter", "gif,jpg"),
        ];
        assert_eq!(query, pairs(&expected), "GIFs come with JPEG stills");
        let (path, query) = target(&heads[1]);
        assert_eq!(path, "/api/v1/test-key/stickers/trending");
        let expected = [
            ("page", "2"),
            ("per_page", "24"),
            ("customer_id", CUSTOMER),
            ("content_filter", "off"),
            ("format_filter", "gif,png"),
        ];
        assert_eq!(
            query,
            pairs(&expected),
            "stickers come with PNG stills (KLIPY has no sticker in JPEG); \
             no text, no locale in English"
        );
        assert!(target(&heads[2]).0.ends_with("/M7ThMWi7.gif"));
        assert!(target(&heads[3]).0.ends_with("/ayXrZ3mBEPbcItVaiams.png"));
    }

    #[test]
    fn maps_429_and_refused_keys() {
        let elsewhere = Elsewhere::new();
        let moved = format!("Location: http://{}/api/v1/moved", elsewhere.addr());
        let invalid = || Reply::json(INVALID_KEY);
        let server = Loopback::serve(|_| {
            vec![
                Reply::status(429),
                Reply::status(401),
                Reply::status(403),
                // KLIPY's real answer to a key it does not know.
                Reply::status(404)
                    .header("Content-Type: application/json")
                    .body(INVALID_KEY.as_bytes()),
                Reply::status(404),
                invalid(),
                Reply::status(400).body(INVALID_KEY.as_bytes()),
                Reply::json(r#"{"result": false, "errors": {"message": ["Busy."]}}"#),
                Reply::status(302).header(&moved),
                Reply::status(500),
                Reply::status(429),
                Reply::status(404),
            ]
        });
        let client = server.client();
        let query = search("cat");
        let service = |failure| Err(BezelError::Service(failure));

        assert_eq!(client.page(&query), service(ServiceFailure::RateLimited));
        assert_eq!(client.page(&query), service(ServiceFailure::KeyRejected));
        assert_eq!(client.page(&query), service(ServiceFailure::KeyRejected));
        let real = client.page(&query);
        assert_eq!(real, service(ServiceFailure::KeyRejected), "real 404");
        let bare = client.page(&query);
        assert_eq!(bare, service(ServiceFailure::KeyRejected), "404 alone");
        let said = client.page(&query);
        assert_eq!(said, service(ServiceFailure::KeyRejected), "200 + message");
        let said = client.page(&query);
        assert_eq!(said, service(ServiceFailure::KeyRejected), "400 + message");
        let other = unavailable_detail(client.page(&query));
        assert_eq!(other, "the answer was not a page of results");
        let redirect = unavailable_detail(client.page(&query));
        assert_eq!(redirect, "HTTP 302: a redirect, not followed");
        assert!(elsewhere.untouched(), "the redirect was not followed");
        assert_eq!(unavailable_detail(client.page(&query)), "HTTP 500");

        // The file host never sees the key: its 404 is no refused key.
        let file = rendition(&format!("{}/ii/example/1001/sm.gif", server.origin()));
        let limited = client.download(&file, PREVIEW_LIMIT);
        let rate_limited = BezelError::Service(ServiceFailure::RateLimited);
        assert_eq!(limited, Err(rate_limited));
        let missing = unavailable_detail(client.download(&file, PREVIEW_LIMIT));
        assert_eq!(missing, "HTTP 404");
        assert_eq!(server.heads().len(), 12);
    }

    #[test]
    fn stops_at_the_byte_limit() {
        let limit = 1000;
        let at_limit = vec![b'G'; 1000];
        let over = vec![b'G'; 1001];
        let big = vec![b'G'; 5000];
        let answer = " ".repeat(usize::try_from(ANSWER_LIMIT).expect("small") + 1);
        let server = Loopback::serve(|_| {
            vec![
                Reply::file(&at_limit).streamed(),
                Reply::file(&over).streamed(),
                Reply::file(&big),
                Reply::json(&answer).streamed(),
            ]
        });
        let client = server.client();
        let file = rendition(&format!("{}/ii/example/1001/hd.gif", server.origin()));
        let too_large = Err(BezelError::InvalidInput(
            "the file is larger than 1000 bytes".into(),
        ));

        let read = client.download(&file, limit).expect("a file at the limit");
        assert_eq!(read.len(), 1000);
        assert_eq!(client.download(&file, limit), too_large, "one byte over");
        assert_eq!(client.download(&file, limit), too_large, "a size told over");
        let page = client.page(&search("cat"));
        assert_eq!(unavailable_detail(page), "the answer was too large");
        assert_eq!(server.heads().len(), 4);
    }

    #[test]
    fn files_only_from_klipy_without_key() {
        let elsewhere = Elsewhere::new();
        let server = Loopback::serve(|_| vec![Reply::file(PIXEL)]);
        let client = server.client();

        let on_host = rendition(&format!("{}/ii/example/1001/sm.gif", server.origin()));
        assert_eq!(client.download(&on_host, PREVIEW_LIMIT), Ok(PIXEL.to_vec()));
        let head = &server.heads()[0];
        assert!(
            head.starts_with("GET /ii/example/1001/sm.gif HTTP/1.1\r\n"),
            "{head}"
        );
        assert!(
            !head.contains(KEY),
            "the key never goes to the file host: {head}"
        );
        assert!(!head.contains(CUSTOMER), "{head}");

        let away = elsewhere.addr();
        let origin = server.origin();
        for location in [
            format!("http://{away}/ii/example/1001/sm.gif"),
            format!("{origin}@{away}/ii/example/1001/sm.gif"),
            format!("{origin}.example/ii/example/1001/sm.gif"),
            origin.clone(),
        ] {
            let refused = client.download(&rendition(&location), PREVIEW_LIMIT);
            let detail = unavailable_detail(refused);
            assert_eq!(detail, "a file outside KLIPY's file host was not read");
        }
        assert!(elsewhere.untouched(), "no connection to another host");
    }

    #[test]
    fn errors_hide_the_key() {
        let closed = {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
            listener.local_addr().expect("its address")
        };
        let echo = format!("{{\"message\": \"invalid key {KEY}\"}}");
        let server = Loopback::serve(|_| {
            vec![
                Reply::status(500).body(echo.as_bytes()),
                Reply::json("{\"result\": true, \"data\": "),
            ]
        });
        let refused = KlipyClient::with_endpoints(KEY, CUSTOMER, Endpoints::loopback(closed));
        let https_only = Endpoints {
            https_only: true,
            ..Endpoints::loopback(server.addr)
        };
        let plain = KlipyClient::with_endpoints(KEY, CUSTOMER, https_only);
        let query = search("cat");

        let failures = [
            server.client().page(&query).map(|_| ()),
            server.client().page(&query).map(|_| ()),
            refused.page(&query).map(|_| ()),
            refused
                .download(&rendition(&format!("http://{closed}/a.gif")), 10)
                .map(|_| ()),
            // ureq's own text for this one holds the whole address.
            plain.page(&query).map(|_| ()),
        ];
        for failure in failures {
            let error = failure.expect_err("a failure");
            for text in [error.to_string(), format!("{error:?}")] {
                assert!(!text.contains(KEY), "{text}");
                assert!(
                    !text.contains("127.0.0.1") && !text.contains("://"),
                    "{text}"
                );
            }
        }
        assert_eq!(
            server.heads().len(),
            2,
            "the HTTPS-only client sent nothing"
        );

        let debug = format!("{:?}", server.client());
        assert!(!debug.contains(KEY) && !debug.contains(CUSTOMER), "{debug}");
        assert_eq!(debug, "KlipyClient { provider: \"klipy\", .. }");
    }

    #[test]
    fn production_is_https_api_klipy_com() {
        let client = KlipyClient::new(KEY, CUSTOMER);
        let production = Endpoints {
            api: "https://api.klipy.com/api/v1".into(),
            files: "https://static.klipy.com".into(),
            https_only: true,
        };
        assert_eq!(client.endpoints, production);
        assert_eq!(client.provider(), "klipy");

        let config = client.agent.config();
        assert!(config.https_only());
        assert_eq!(config.max_redirects(), 0);
        assert!(!config.http_status_as_error());
        assert_eq!(config.timeouts().global, Some(Duration::from_secs(10)));
        assert_eq!(config.tls_config().provider(), TlsProvider::Rustls);
        assert!(matches!(
            config.tls_config().root_certs(),
            RootCerts::WebPki
        ));

        let stickers = GifQuery::new(GifKind::Sticker, "", 1, Explicit::Hidden, Language::English);
        assert_eq!(
            client.page_address(&search("cat")),
            "https://api.klipy.com/api/v1/test-key/gifs/search"
        );
        assert_eq!(
            client.page_address(&stickers),
            "https://api.klipy.com/api/v1/test-key/stickers/trending"
        );
        let odd = KlipyClient::new("a/b?c=d#e", CUSTOMER);
        assert_eq!(
            odd.page_address(&search("cat")),
            "https://api.klipy.com/api/v1/a%2Fb%3Fc%3Dd%23e/gifs/search",
            "a key stays one path segment"
        );

        let file = "https://static.klipy.com/ii/example/1001/hd.gif";
        assert_eq!(client.file_address(file), Ok(file));
        for elsewhere in [
            "http://static.klipy.com/ii/example/1001/hd.gif",
            "https://static.klipy.com.example/ii/example/1001/hd.gif",
            "https://static.klipy.com@example.com/ii/example/1001/hd.gif",
            "https://static.klipy.com:8443/ii/example/1001/hd.gif",
            "https://api.klipy.com/ii/example/1001/hd.gif",
        ] {
            assert!(client.file_address(elsewhere).is_err(), "{elsewhere}");
        }
    }

    #[test]
    fn customer_ids_are_128_random_bits() {
        let one = new_customer_id().expect("an id");
        let two = new_customer_id().expect("an id");
        assert_eq!(one.len(), 32);
        assert!(
            one.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert_ne!(one, two);
    }

    /// The production client against KLIPY itself, with the user's key
    /// read from `BEZEL_KLIPY_KEY` and never printed: five requests (GIF
    /// trending, a sticker search, a sticker's GIF and PNG still, a GIF's
    /// JPEG still). Run by hand only:
    /// `cargo test -p bezel-klipy --lib -- --ignored --exact
    /// client::tests::real_klipy_answers_with_the_users_key`.
    #[test]
    #[ignore = "asks api.klipy.com with the key in BEZEL_KLIPY_KEY"]
    fn real_klipy_answers_with_the_users_key() {
        let key = std::env::var("BEZEL_KLIPY_KEY").unwrap_or_default();
        let key = key.trim();
        if key.is_empty() {
            eprintln!("BEZEL_KLIPY_KEY is not set: nothing was asked of KLIPY");
            return;
        }
        let customer = new_customer_id().expect("a customer id");
        let client = KlipyClient::new(key, &customer);

        let trending = GifQuery::new(GifKind::Gif, "", 1, Explicit::Hidden, Language::English);
        let gifs = app::gifs::search(&client, &trending).expect("trending GIFs");
        assert!(!gifs.items.is_empty(), "no trending GIF");
        assert!(gifs.items.iter().all(|i| i.kind == GifKind::Gif));
        let gif = &gifs.items[0];
        let still_of = |i: &GifItem| i.preview(Motion::Still).map(|r| r.format);
        assert!(
            gifs.items
                .iter()
                .all(|i| still_of(i) == Some(RenditionFormat::Jpeg))
        );

        let cats = GifQuery::new(
            GifKind::Sticker,
            "cat",
            1,
            Explicit::Hidden,
            Language::English,
        );
        let stickers = app::gifs::search(&client, &cats).expect("cat stickers");
        assert!(!stickers.items.is_empty(), "no cat sticker");
        assert!(stickers.items.iter().all(|i| i.kind == GifKind::Sticker));
        assert!(
            stickers
                .items
                .iter()
                .all(|i| still_of(i) == Some(RenditionFormat::Png)),
            "every sticker stands still as a PNG"
        );
        let sticker = &stickers.items[0];

        let moving = app::gifs::preview(&client, sticker, Motion::Animated);
        let moving = moving.expect("the sticker's GIF").expect("a GIF");
        assert!(moving.starts_with(b"GIF8") && is_gif(&moving), "GIF magic");
        let still = app::gifs::preview(&client, sticker, Motion::Still);
        let still = still.expect("the sticker's still").expect("a still");
        assert_eq!(RenditionFormat::of(&still), Some(RenditionFormat::Png));
        let still = app::gifs::preview(&client, gif, Motion::Still);
        let still = still.expect("the GIF's still").expect("a still");
        assert_eq!(RenditionFormat::of(&still), Some(RenditionFormat::Jpeg));
    }

    #[test]
    fn locale_and_filter_follow_the_query() {
        assert_eq!(locale(Language::PortugueseBr), Some("BR"));
        assert_eq!(locale(Language::English), None);
        assert_eq!(filter_name(Explicit::Hidden.filter()), "medium");
        assert_eq!(filter_name(Explicit::Shown.filter()), "off");
        assert_eq!(format_filter(GifKind::Gif), "gif,jpg");
        assert_eq!(format_filter(GifKind::Sticker), "gif,png");
        assert_eq!(path_segment("Ab0_-"), "Ab0_-");
        assert_eq!(path_segment("é ."), "%C3%A9%20%2E");
    }
}
