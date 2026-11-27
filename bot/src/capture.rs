//! In-process packet capture, interception, and direct two-way packet injection.

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use parking_lot::{Mutex, RwLock};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub const LOG_CAPACITY: usize = 1000;
const MAX_INFLATED: u64 = 16 * 1024 * 1024;

/// Direct socket injection sink provided by `grimlite_frontend::capture::CaptureNavigator`.
pub trait PacketSink: Send + Sync {
    fn send_to_server(&self, bytes: Vec<u8>) -> Result<(), String>;
    fn send_to_client(&self, bytes: Vec<u8>) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageInfo {
    /// Inflated UTF-8 packet text. Mutate to tamper with the packet in flight.
    pub content: String,
    /// Set to `false` to drop/block the packet.
    pub send: bool,
}

impl MessageInfo {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            send: true,
        }
    }
}

/// Sees, edits, or drops packets between the AQW client and server.
pub trait Interceptor: Send + Sync {
    fn priority(&self) -> i32 {
        0
    }

    /// `outbound` is `true` for client -> server, `false` for server -> client.
    fn intercept(&self, message: &mut MessageInfo, outbound: bool);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterceptedPacket {
    pub timestamp: String,
    pub packet: String,
    /// `Some(true)` = OUT (client -> server), `Some(false)` = IN (server -> client), `None` = BLOCKED.
    pub outbound: Option<bool>,
}

impl InterceptedPacket {
    pub fn new(packet: String, outbound: Option<bool>) -> Self {
        Self {
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            packet,
            outbound,
        }
    }

    pub fn direction(&self) -> &'static str {
        match self.outbound {
            Some(true) => "OUT",
            Some(false) => "IN",
            None => "BLOCK",
        }
    }
}

pub struct CaptureProxy {
    running: AtomicBool,
    logging: AtomicBool,
    interceptors: RwLock<Vec<Arc<dyn Interceptor>>>,
    sink: RwLock<Option<Arc<dyn PacketSink>>>,
    log: Mutex<VecDeque<InterceptedPacket>>,
    logged: AtomicU64,
    version: AtomicU64,
}

impl Default for CaptureProxy {
    fn default() -> Self {
        Self {
            running: AtomicBool::new(false),
            logging: AtomicBool::new(false),
            interceptors: RwLock::new(Vec::new()),
            sink: RwLock::new(None),
            log: Mutex::new(VecDeque::new()),
            logged: AtomicU64::new(0),
            version: AtomicU64::new(0),
        }
    }
}

impl CaptureProxy {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn start(&self) {
        self.running.store(true, Ordering::Release);
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn is_logging(&self) -> bool {
        self.logging.load(Ordering::Relaxed)
    }

    pub fn set_logging(&self, on: bool) {
        self.logging.store(on, Ordering::Relaxed);
    }

    pub fn set_sink(&self, sink: Arc<dyn PacketSink>) {
        *self.sink.write() = Some(sink);
    }

    pub fn clear_sink(&self) {
        *self.sink.write() = None;
    }

    pub fn is_socket_connected(&self) -> bool {
        self.sink.read().is_some()
    }

    /// Injects a raw packet directly to the AQW server socket (NUL-terminated automatically).
    pub fn send_to_server(&self, packet: &str) -> Result<(), String> {
        let sink = self
            .sink
            .read()
            .clone()
            .ok_or_else(|| "game socket is not connected".to_string())?;
        let trimmed = packet.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let mut bytes = trimmed.as_bytes().to_vec();
        if bytes.last().copied() != Some(0) {
            bytes.push(0);
        }
        sink.send_to_server(bytes)?;
        if self.is_logging() {
            self.log_packet(InterceptedPacket::new(trimmed.to_string(), Some(true)));
        }
        Ok(())
    }

    /// Injects a simulated server packet directly into the AQW client socket.
    pub fn send_to_client(&self, packet: &str) -> Result<(), String> {
        let sink = self
            .sink
            .read()
            .clone()
            .ok_or_else(|| "game socket is not connected".to_string())?;
        let trimmed = packet.trim();
        if trimmed.is_empty() {
            return Ok(());
        }
        let mut bytes = trimmed.as_bytes().to_vec();
        if bytes.last().copied() != Some(0) {
            bytes.push(0);
        }
        sink.send_to_client(bytes)?;
        if self.is_logging() {
            self.log_packet(InterceptedPacket::new(trimmed.to_string(), Some(false)));
        }
        Ok(())
    }

    pub fn add_interceptor(&self, interceptor: Arc<dyn Interceptor>) {
        let mut list = self.interceptors.write();
        list.push(interceptor);
        list.sort_by_key(|i| i.priority());
    }

    pub fn remove_interceptor(&self, interceptor: &Arc<dyn Interceptor>) {
        self.interceptors
            .write()
            .retain(|i| !Arc::ptr_eq(i, interceptor));
    }

    pub fn snapshot(&self) -> (u64, Vec<InterceptedPacket>) {
        let log = self.log.lock();
        let first = self.logged.load(Ordering::Relaxed) - log.len() as u64;
        (first, log.iter().cloned().collect())
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Relaxed)
    }

    pub fn clear(&self) {
        self.log.lock().clear();
        self.version.fetch_add(1, Ordering::Relaxed);
    }

    /// Processes a single NUL-stripped packet. Returns `None` if blocked.
    pub fn process<'a>(&self, data: &'a [u8], outbound: bool) -> Option<Cow<'a, [u8]>> {
        if !self.is_running() {
            return Some(Cow::Borrowed(data));
        }
        let logging = self.is_logging();
        let interceptors = self.interceptors.read().clone();
        if interceptors.is_empty() && !logging {
            return Some(Cow::Borrowed(data));
        }

        let raw = String::from_utf8_lossy(data);
        let inflated = if outbound { None } else { unpack(&raw) };
        let compressed = inflated.is_some();
        let content = inflated.unwrap_or_else(|| raw.into_owned());
        let mut message = MessageInfo::new(content.clone());

        for interceptor in &interceptors {
            interceptor.intercept(&mut message, outbound);
        }

        if logging {
            self.log_packet(InterceptedPacket::new(
                message.content.clone(),
                message.send.then_some(outbound),
            ));
        }

        if !message.send {
            None
        } else if message.content == content {
            Some(Cow::Borrowed(data))
        } else if compressed {
            Some(Cow::Owned(pack(&message.content)))
        } else {
            Some(Cow::Owned(message.content.into_bytes()))
        }
    }

    fn log_packet(&self, packet: InterceptedPacket) {
        let mut log = self.log.lock();
        if log.len() >= LOG_CAPACITY {
            log.pop_front();
        }
        log.push_back(packet);
        self.logged.fetch_add(1, Ordering::Relaxed);
        self.version.fetch_add(1, Ordering::Relaxed);
    }
}

/// Splits raw TCP chunks into NUL-terminated SmartFox packets and passes each through `CaptureProxy`.
#[derive(Default)]
pub struct Stream {
    pending: Vec<u8>,
}

impl Stream {
    pub fn filter(&mut self, proxy: &CaptureProxy, chunk: Vec<u8>, outbound: bool) -> Vec<u8> {
        self.pending.extend_from_slice(&chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.pending.iter().position(|&b| b == 0) {
            let packet: Vec<u8> = self.pending.drain(..=pos).collect();
            let body = &packet[..packet.len() - 1];
            if body.is_empty() {
                continue;
            }
            if let Some(processed) = proxy.process(body, outbound) {
                out.extend_from_slice(&processed);
                out.push(0);
            }
        }
        out
    }
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Decompresses AQW's compressed server packets: `'Z'` followed by Base64-encoded zlib data.
pub fn unpack(raw: &str) -> Option<String> {
    let body = raw.strip_prefix('Z')?;
    let body = body.trim_end_matches('=');
    if body.len() * 3 / 4 < 8 || body.chars().any(|c| c as u32 > 255) {
        return None;
    }
    let mut bytes = Vec::with_capacity(body.len() * 3 / 4);
    let (mut bits, mut count) = (0u32, 0u32);
    for b in body.bytes() {
        let value = BASE64.iter().position(|&c| c == b).unwrap_or(0) as u32;
        bits = (bits << 6) | value;
        count += 6;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
        }
    }
    let mut inflated = Vec::new();
    ZlibDecoder::new(&bytes[..])
        .take(MAX_INFLATED + 1)
        .read_to_end(&mut inflated)
        .ok()?;
    if inflated.len() as u64 > MAX_INFLATED {
        return None;
    }
    Some(String::from_utf8_lossy(&inflated).into_owned())
}

/// Re-compresses an edited server packet back into AQW's `'Z'` + Base64(zlib) wire format.
pub fn pack(text: &str) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = encoder.write_all(text.as_bytes());
    let data = encoder.finish().unwrap_or_default();
    let mut out = Vec::with_capacity(1 + data.len().div_ceil(3) * 4);
    out.push(b'Z');
    for chunk in data.chunks(3) {
        let n = ((chunk[0] as u32) << 16)
            | ((*chunk.get(1).unwrap_or(&0) as u32) << 8)
            | (*chunk.get(2).unwrap_or(&0) as u32);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(BASE64[((n >> (18 - 6 * i)) as usize) & 63]);
            } else {
                out.push(b'=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DropContains(&'static str);
    impl Interceptor for DropContains {
        fn intercept(&self, message: &mut MessageInfo, _outbound: bool) {
            if message.content.contains(self.0) {
                message.send = false;
            }
        }
    }

    #[test]
    fn stream_splits_and_blocks_packets() {
        let proxy = CaptureProxy::default();
        proxy.start();
        proxy.set_logging(true);
        proxy.add_interceptor(Arc::new(DropContains("secret")));

        let mut stream = Stream::default();
        let out = stream.filter(&proxy, b"%xt%zm%hi%1%\0%xt%zm%secret%1%\0".to_vec(), true);
        assert_eq!(out, b"%xt%zm%hi%1%\0");
        let (_, logged) = proxy.snapshot();
        assert_eq!(logged.len(), 2);
        assert_eq!(logged[0].direction(), "OUT");
        assert_eq!(logged[1].direction(), "BLOCK");
    }

    #[test]
    fn unpacks_and_packs_aqw_z_base64_zlib_packets() {
        let sample = r#"{"t":"xt","b":{"r":-1,"o":{"cmd":"moveToArea","areaId":121135,"strMapName":"battleon"}}}"#;
        let packed = pack(sample);
        let packed_str = String::from_utf8(packed).unwrap();
        assert!(packed_str.starts_with('Z'));
        let unpacked = unpack(&packed_str).expect("should unpack Z+base64+zlib");
        assert_eq!(unpacked, sample);
    }
}
