use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::net::{TcpListener, TcpStream};

pub enum OptionalListener {
    None,
    Some(TcpListener),
}

pub enum OptionalListenerFuture<'l> {
    None,
    Some(&'l TcpListener),
}

impl OptionalListener {
    pub fn accept<'l>(&'l self) -> OptionalListenerFuture<'l> {
        match self {
            OptionalListener::None => OptionalListenerFuture::None,
            OptionalListener::Some(listener) => OptionalListenerFuture::Some(&listener),
        }
    }
}

impl<'l> Future for OptionalListenerFuture<'l> {
    type Output = io::Result<(TcpStream, SocketAddr)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match *self {
            OptionalListenerFuture::None => Poll::Pending,
            OptionalListenerFuture::Some(f) => f.poll_accept(cx),
        }
    }
}
