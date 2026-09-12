use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use poison_swamp_level::handler::{App, HandlerType, MetricApp, preflight, proxy};
use poison_swamp_level::{Classifier, Config, Garbage, ServerMode, init_logger};
use poison_swamp_level::{metrics, metrics::Metrics};
use std::{
    error::Error,
    io::{self, IsTerminal},
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
};
use tokio::net::{TcpListener, TcpStream};
#[cfg(unix)]
use tokio::{
    signal::unix::{SignalKind, signal},
    sync::{mpsc, mpsc::Sender},
};

#[tokio::main(flavor = "local")]
async fn main() {
    let config = match Config::read_from_file("./config.kdl") {
        Ok(config) => config,
        Err(e) => {
            if io::stderr().is_terminal() {
                eprintln!("{}", e.explain());
            } else {
                eprintln!("{e}");
            }
            std::process::exit(1);
        }
    };

    init_logger(&config);

    let mut app_config = AppConfig::from_config(config).unwrap();
    let mut listener = app_config.listen().await.unwrap();
    let metrics_listener =
        TcpListener::bind(SocketAddr::new("127.0.0.1".parse().unwrap(), 4242))
            .await
            .unwrap();

    #[cfg(unix)]
    let mut sighup = signal(SignalKind::hangup()).unwrap();
    #[cfg(unix)]
    let (config_reload_tx, mut config_reload) = mpsc::channel::<AppConfig>(1);

    loop {
        #[cfg(not(unix))]
        tokio::select! {
            listen_result = listener.accept() => {
                handle_connection(&app_config, listen_result);
            },
            metrics_listen_result = metrics_listener.accept() => {
                handle_metrics(&app_config, metrics_listen_result);
            },
        }
        #[cfg(unix)]
        tokio::select! {
            listen_result = listener.accept() => {
                handle_connection(&app_config, listen_result);
            },
            metrics_listen_result = metrics_listener.accept() => {
                handle_metrics(&app_config, metrics_listen_result);
            },
            Some(new_app_config) = config_reload.recv() => {
                if app_config.listen_addr() != new_app_config.listen_addr() {
                    match new_app_config.listen().await {
                        Ok(new_listener) => listener = new_listener,
                        Err(e) => {
                            log::error!("Config reload encountered error: {e}");
                            continue;
                        }
                    }
                }
                app_config = new_app_config;

                log::info!("Reloaded config");
            },
            _ = sighup.recv() => {
                reload_config(&app_config, config_reload_tx.clone());
            },
        }
    }
}

struct AppConfig {
    config: Config,
    classifier: Arc<Classifier>,
    garbage: Arc<Garbage>,
    handler: HandlerType,
    metrics: Arc<Mutex<Metrics>>,
}

impl AppConfig {
    fn from_config(config: Config) -> Result<Self, Box<dyn Error>> {
        let classifier = Classifier::new(&config)?;
        let garbage = Garbage::new(&config)?;
        let metrics = metrics::init(&config);

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

    fn to_service(&self, client_ip: IpAddr) -> App {
        App {
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

    fn to_metric_service(&self) -> MetricApp {
        MetricApp {
            metrics: self.metrics.clone(),
        }
    }

    fn listen_addr(&self) -> SocketAddr {
        self.config.server.listen
    }

    fn same_as(&self, other: &Config) -> bool {
        self.config == *other
    }

    async fn listen(&self) -> io::Result<TcpListener> {
        let listener = TcpListener::bind(self.listen_addr()).await?;
        log::info!("Listening on {}", self.listen_addr());
        Ok(listener)
    }
}

fn handle_connection(app_config: &AppConfig, result: io::Result<(TcpStream, SocketAddr)>) {
    match result {
        Ok((stream, addr)) => {
            let io = TokioIo::new(stream);
            let app = app_config.to_service(addr.ip());
            tokio::task::spawn(async move {
                if let Err(e) = http1::Builder::new().serve_connection(io, app).await {
                    println!("Failed to serve connection: {e}");
                }
            });
        }
        Err(e) => log::error!("error handling connection: {e}"),
    }
}

fn handle_metrics(app_config: &AppConfig, result: io::Result<(TcpStream, SocketAddr)>) {
    match result {
        Ok((stream, _addr)) => {
            let io = TokioIo::new(stream);
            let app = app_config.to_metric_service();
            tokio::task::spawn(async move {
                if let Err(e) = http1::Builder::new().serve_connection(io, app).await {
                    println!("Failed to serve metrics: {e}");
                }
            });
        }
        Err(e) => log::error!("error handling metrics: {e}"),
    }
}

#[cfg(unix)]
fn reload_config(existing: &AppConfig, tx: Sender<AppConfig>) {
    let new_config = match Config::read_from_file("./config.kdl") {
        Ok(config) => config,
        Err(e) => {
            log::error!("Config reload encountered error: {e}");
            return;
        }
    };

    if existing.same_as(&new_config) {
        log::info!("Reloaded config: no changes");
        return;
    }

    std::thread::spawn(move || match AppConfig::from_config(new_config) {
        Ok(app_config) => tx.blocking_send(app_config),
        Err(e) => Ok({
            log::error!("Config reload encountered error: {e}");
        }),
    });
}
