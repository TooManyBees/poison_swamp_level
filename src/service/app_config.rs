use crate::classifier::Classifier;
use crate::config::{Config, ServerMode};
use crate::garbage::Garbage;
use crate::metrics::{Metrics, init as init_metrics};
use crate::service::OptionalListener;
use crate::service::handler::{HandlerType, MetricsHandler, PslHandler, preflight, proxy};
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::server::graceful::GracefulShutdown;
use std::{
    error::Error,
    io,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
};
use tokio::net::{TcpListener, TcpStream};

pub struct AppConfig {
    config: Config,
    classifier: Arc<Classifier>,
    garbage: Arc<Garbage>,
    handler: HandlerType,
    metrics: Arc<Mutex<Metrics>>,
}

impl AppConfig {
    pub fn from_config(config: Config) -> Result<Self, Box<dyn Error>> {
        let classifier = Classifier::new(&config)?;
        let garbage = Garbage::new(&config)?;
        let metrics = init_metrics(&config);

        let handler = match config.server.mode {
            ServerMode::Proxy => proxy,
            ServerMode::Preflight => preflight,
        };

        Ok(AppConfig {
            config,
            classifier: Arc::new(classifier),
            garbage: Arc::new(garbage),
            handler,
            metrics,
        })
    }

    pub fn replace_metrics(&mut self, other: &AppConfig) {
        self.metrics = other.metrics.clone();
    }

    pub fn to_service(&self, client_ip: IpAddr) -> PslHandler {
        PslHandler {
            client_ip,
            classifier: self.classifier.clone(),
            garbage: self.garbage.clone(),
            handler: self.handler,
            status_code_valid: self.config.server.status_code_valid,
            status_code_spam: self.config.server.status_code_spam,
            logging: self.config.logging.request_handler,
            metrics: self.metrics.clone(),
        }
    }

    pub fn to_metric_service(&self) -> MetricsHandler {
        MetricsHandler {
            metrics: self.metrics.clone(),
        }
    }

    pub fn listen_addr(&self) -> SocketAddr {
        self.config.server.listen
    }

    pub fn listen_metrics_addr(&self) -> Option<SocketAddr> {
        self.config.metrics.as_ref().map(|metrics| metrics.listen)
    }

    pub fn same_as(&self, other: &Config) -> bool {
        self.config == *other
    }

    pub async fn listen(&self) -> io::Result<TcpListener> {
        match TcpListener::bind(self.listen_addr()).await {
            Ok(l) => {
                log::info!("Listening on {}", self.listen_addr());
                Ok(l)
            }
            Err(e) => {
                log::error!("Could not listen on {}: {}", self.listen_addr(), e);
                Err(e)
            }
        }
    }

    pub async fn listen_metrics(&self) -> io::Result<OptionalListener> {
        let addr = match self.listen_metrics_addr() {
            None => return Ok(OptionalListener::None),
            Some(addr) => addr,
        };

        match TcpListener::bind(addr).await {
            Ok(l) => {
                log::info!("Metrics listening on {}", addr);
                Ok(OptionalListener::Some(l))
            }
            Err(e) => {
                log::error!("Could not listen on {}: {}", addr, e);
                Err(e)
            }
        }
    }

    pub fn handle_connection(
        &self,
        result: io::Result<(TcpStream, SocketAddr)>,
        graceful: &GracefulShutdown,
    ) {
        match result {
            Ok((stream, addr)) => {
                let io = TokioIo::new(stream);
                let app = self.to_service(addr.ip());
                let conn = http1::Builder::new().serve_connection(io, app);
                let fut = graceful.watch(conn);
                tokio::task::spawn(async move {
                    if let Err(e) = fut.await {
                        println!("Failed to serve connection: {e}");
                    }
                });
            }
            Err(e) => log::error!("error handling connection: {e}"),
        }
    }

    pub fn handle_metrics(
        &self,
        result: io::Result<(TcpStream, SocketAddr)>,
        graceful: &GracefulShutdown,
    ) {
        match result {
            Ok((stream, _addr)) => {
                let io = TokioIo::new(stream);
                let app = self.to_metric_service();
                let conn = http1::Builder::new().serve_connection(io, app);
                let fut = graceful.watch(conn);
                tokio::task::spawn(async move {
                    if let Err(e) = fut.await {
                        println!("Failed to serve metrics: {e}");
                    }
                });
            }
            Err(e) => log::error!("error handling metrics: {e}"),
        }
    }

    pub fn persist_metrics(self) {
        self.metrics.lock().unwrap().persist();
    }
}
