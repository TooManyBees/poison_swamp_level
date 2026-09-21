use std::fs::{Permissions, set_permissions};
use std::future::Future;
use std::net::{AddrParseError, IpAddr, SocketAddr};
use std::os::unix::fs::PermissionsExt;
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use std::{fmt, io};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};

#[derive(Debug)]
pub enum Listener {
    Tcp(TcpListener),
    Unix(UnixListener),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Address {
    TcpSocket(SocketAddr),
    UnixSocket(String),
}

impl Address {
    pub fn from_str(s: &str) -> Result<Address, AddrParseError> {
        if let Some(path) = s.strip_prefix("unix:") {
            Ok(Address::UnixSocket(path.to_string()))
        } else {
            Ok(Address::TcpSocket(SocketAddr::from_str(s)?))
        }
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Address::TcpSocket(s) => s.fmt(f),
            Address::UnixSocket(s) => s.fmt(f),
        }
    }
}

impl Listener {
    pub async fn bind(addr: &Address) -> io::Result<Listener> {
        match addr {
            Address::TcpSocket(addr) => TcpListener::bind(addr).await.map(Listener::Tcp),
            Address::UnixSocket(path) => {
                let listener = UnixListener::bind(&path)?;
                set_permissions(path, Permissions::from_mode(0o666))?;
                Ok(Listener::Unix(listener))
            }
        }
    }

    pub async fn accept(&self) -> io::Result<Stream> {
        match self {
            Listener::Tcp(l) => l.accept().await.map(|(s, a)| Stream::Tcp(s, a)),
            Listener::Unix(l) => l.accept().await.map(|(s, _a)| Stream::Unix(s)),
        }
    }

    fn poll_accept(&self, ctx: &mut Context<'_>) -> Poll<io::Result<Stream>> {
        match self {
            Listener::Tcp(l) => match l.poll_accept(ctx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Ok((l, s))) => Poll::Ready(Ok(Stream::Tcp(l, s))),
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            },
            Listener::Unix(l) => match l.poll_accept(ctx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Ok((l, _s))) => Poll::Ready(Ok(Stream::Unix(l))),
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            },
        }
    }
}

pub enum Stream {
    Tcp(TcpStream, SocketAddr),
    Unix(UnixStream),
}

impl Stream {
    pub fn client_ip(&self) -> Option<IpAddr> {
        match self {
            Stream::Tcp(_stream, addr) => Some(addr.ip()),
            Stream::Unix(_) => None,
        }
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Stream>,
        ctx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Tcp(stream, _) => Pin::new(stream).poll_read(ctx, buf),
            Stream::Unix(stream) => Pin::new(stream).poll_read(ctx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        ctx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Tcp(stream, _) => Pin::new(stream).poll_write(ctx, buf),
            Stream::Unix(stream) => Pin::new(stream).poll_write(ctx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Tcp(stream, _) => Pin::new(stream).poll_flush(ctx),
            Stream::Unix(stream) => Pin::new(stream).poll_flush(ctx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Tcp(stream, _) => Pin::new(stream).poll_shutdown(ctx),
            Stream::Unix(stream) => Pin::new(stream).poll_shutdown(ctx),
        }
    }
}

pub enum OptionalListener {
    None,
    Some(Listener),
}

pub enum OptionalListenerFuture<'l> {
    None,
    Some(&'l Listener),
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
    type Output = io::Result<Stream>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match *self {
            OptionalListenerFuture::None => Poll::Pending,
            OptionalListenerFuture::Some(f) => f.poll_accept(cx),
        }
    }
}
