//! Wraps Ruffle's `NavigatorBackend::connect_socket` to route all AQW game packets through
//! `grimlite_bot::capture::CaptureProxy` in-process (no local TCP proxy or reconnect needed).

use async_channel::{Receiver, Sender};
use grimlite_bot::capture::{CaptureProxy, Stream};
use ruffle_core::backend::navigator::{
    ErrorResponse, NavigationMethod, NavigatorBackend, OwnedFuture, Request, SuccessResponse,
};
use ruffle_core::indexmap::IndexMap;
use ruffle_core::loader::Error;
use ruffle_core::socket::{SocketAction, SocketHandle};
use std::sync::Arc;
use std::time::Duration;
use url::{ParseError, Url};

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

        let outbound_capture = capture.clone();
        self.inner.spawn_future(Box::pin(async move {
            let mut stream = Stream::default();
            while let Ok(chunk) = receiver.recv().await {
                let bytes = stream.filter(&outbound_capture, chunk, true);
                if !bytes.is_empty() && to_server.send(bytes).await.is_err() {
                    break;
                }
            }
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
            Ok(())
        }));
    }
}
