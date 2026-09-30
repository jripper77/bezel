//! Ports: the traits adapters implement (driven) or call (driving).

use crate::Result;
use crate::domain::discovery::Endpoint;

/// Driven port: enumerates the USB endpoints the host can see, without
/// opening or writing to any of them.
pub trait DeviceBus {
    /// Every candidate endpoint currently connected. Adapters may pre-filter to
    /// the catalog's USB ids; unknown endpoints are ignored by the core anyway.
    fn endpoints(&self) -> Result<Vec<Endpoint>>;
}
