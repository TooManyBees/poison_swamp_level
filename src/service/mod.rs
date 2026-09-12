mod app_config;
mod handler;

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::{TcpListener, TcpStream};

pub use app_config::AppConfig;

pub enum OptionalListener {
    None,
    Some(TcpListener),
}

pub enum TcpListenerFuture<'l> {
    None,
    Some(&'l TcpListener),
}

impl OptionalListener {
    pub fn accept<'l>(&'l self) -> TcpListenerFuture<'l> {
        match self {
            OptionalListener::None => TcpListenerFuture::None,
            OptionalListener::Some(listener) => TcpListenerFuture::Some(&listener),
        }
    }
}

impl<'l> Future for TcpListenerFuture<'l> {
    type Output = io::Result<(TcpStream, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match *self {
            TcpListenerFuture::None => Poll::Pending,
            TcpListenerFuture::Some(f) => f.poll_accept(cx),
        }
    }
}
