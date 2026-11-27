//! Shared Ruffle <-> Grimlite Ri bot bridge (no egui, winit, or rfd).

pub mod capture;
pub mod external;
#[cfg(feature = "wgpu")]
pub mod gpu;
pub mod ruffle_host;

use std::path::Path;
use std::sync::{Arc, OnceLock};

/// Default disk cache cap (1 GB) for AQW SWFs (`Game.swf`, maps, monsters, player gear).
const DEFAULT_HTTP_CACHE_BYTES: u64 = 1024 * 1024 * 1024;

/// Returns a process-wide [`ruffle_http_cache::HttpCache`] rooted at `cache_dir`, so AQW's CDN
/// assets (`Cache-Control: public, max-age=2678400` + `ETag`) are served directly from disk across
/// tabs and sessions instead of re-downloading & decompressing over HTTPS.
pub fn shared_http_cache(cache_dir: &Path) -> Option<Arc<ruffle_http_cache::HttpCache>> {
    static CACHE: OnceLock<Option<Arc<ruffle_http_cache::HttpCache>>> = OnceLock::new();
    CACHE
        .get_or_init(|| match ruffle_http_cache::HttpCache::new(cache_dir, DEFAULT_HTTP_CACHE_BYTES) {
            Ok(cache) => Some(cache),
            Err(e) => {
                tracing::warn!("SWF HTTP cache disabled at {}: {e}", cache_dir.display());
                None
            }
        })
        .clone()
}
