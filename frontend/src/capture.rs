//! Wraps Ruffle's `NavigatorBackend::connect_socket` to route all AQW game packets through
//! `grimlite_bot::capture::CaptureProxy` in-process and expose direct two-way packet injection.

use async_channel::{Receiver, Sender};
use grimlite_bot::capture::{CaptureProxy, PacketSink, Stream};
use ruffle_core::backend::navigator::{
    ErrorResponse, NavigationMethod, NavigatorBackend, OwnedFuture, Request, SuccessResponse,
};
use ruffle_core::indexmap::IndexMap;
use ruffle_core::loader::Error;
use ruffle_core::socket::{SocketAction, SocketHandle};
use std::sync::Arc;
use std::time::Duration;
use url::{ParseError, Url};

struct RuffleSocketSink {
    handle: SocketHandle,
    to_server: Sender<Vec<u8>>,
    to_client: Sender<SocketAction>,
}

impl PacketSink for RuffleSocketSink {
    fn send_to_server(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.to_server
            .try_send(bytes)
            .map_err(|e| format!("failed to send packet to server: {e}"))
    }

    fn send_to_client(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.to_client
            .try_send(SocketAction::Data(self.handle, bytes))
            .map_err(|e| format!("failed to inject packet to client: {e}"))
    }
}

pub struct CaptureNavigator<N> {
    inner: N,
    capture: Option<Arc<CaptureProxy>>,
}

impl<N> CaptureNavigator<N> {
    pub fn new(inner: N, capture: Option<Arc<CaptureProxy>>) -> Self {
        Self { inner, capture }
    }
}

impl<N: NavigatorBackend> NavigatorBackend for CaptureNavigator<N> {
    fn navigate_to_url(
        &self,
        url: &str,
        target: &str,
        vars_method: Option<(NavigationMethod, IndexMap<String, String>)>,
    ) {
        self.inner.navigate_to_url(url, target, vars_method)
    }

    fn fetch(&self, request: Request) -> OwnedFuture<Box<dyn SuccessResponse>, ErrorResponse> {
        self.inner.fetch(request)
    }

    fn resolve_url(&self, url: &str) -> Result<Url, ParseError> {
        self.inner.resolve_url(url)
    }

    fn spawn_future(&mut self, future: OwnedFuture<(), Error>) {
        self.inner.spawn_future(future)
    }

    fn pre_process_url(&self, url: Url) -> Url {
        self.inner.pre_process_url(url)
    }

    fn connect_socket(
        &mut self,
        host: String,
        port: u16,
        timeout: Duration,
        handle: SocketHandle,
        receiver: Receiver<Vec<u8>>,
        sender: Sender<SocketAction>,
    ) {
        let Some(capture) = self.capture.clone() else {
            return self
                .inner
                .connect_socket(host, port, timeout, handle, receiver, sender);
        };

        let (to_server, server_rx) = async_channel::unbounded();
        let (server_tx, from_server) = async_channel::unbounded();
        self.inner
            .connect_socket(host, port, timeout, handle, server_rx, server_tx);

        // Register direct injection sink for Packet Spammer / Tamperer / Bot commands.
        capture.set_sink(Arc::new(RuffleSocketSink {
            handle,
            to_server: to_server.clone(),
            to_client: sender.clone(),
        }));

        let outbound_capture = capture.clone();
        self.inner.spawn_future(Box::pin(async move {
            let mut stream = Stream::default();
            while let Ok(chunk) = receiver.recv().await {
                let bytes = stream.filter(&outbound_capture, chunk, true);
                if !bytes.is_empty() && to_server.send(bytes).await.is_err() {
                    break;
                }
            }
            outbound_capture.clear_sink();
            Ok(())
        }));

        self.inner.spawn_future(Box::pin(async move {
            let mut stream = Stream::default();
            while let Ok(action) = from_server.recv().await {
                let action = match action {
                    SocketAction::Data(handle, chunk) => {
                        let bytes = stream.filter(&capture, chunk, false);
                        if bytes.is_empty() {
                            continue;
                        }
                        SocketAction::Data(handle, bytes)
                    }
                    other => other,
                };
                if sender.send(action).await.is_err() {
                    break;
                }
            }
            capture.clear_sink();
            Ok(())
        }));
    }
}
