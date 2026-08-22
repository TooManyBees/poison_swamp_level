use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use poison_swamp_level::handler::{App, HandlerType, preflight, proxy};
use poison_swamp_level::{Classifier, Config, Garbage, ServerMode};
use std::{
    error::Error,
    net::{IpAddr, SocketAddr},
    sync::Arc,
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
            eprintln!("{}", e.explain());
            std::process::exit(1);
        }
    };

    env_logger::builder()
        .filter(None, config.logging.level)
        .init();

    let mut app_config = AppConfig::from_config(config).unwrap();
    let mut listener = app_config.listen().await.unwrap();

    #[cfg(unix)]
    let mut sighup = signal(SignalKind::hangup()).unwrap();
    #[cfg(unix)]
    let (config_reload_tx, mut config_reload) = mpsc::channel::<AppConfig>(1);

    loop {
        #[cfg(not(unix))]
        if let Ok(stream_addr) = listener.accept().await {
            handle_connection(&app_config, stream_addr);
        }
        #[cfg(unix)]
        tokio::select! {
            Ok(stream_addr) = listener.accept() => {
                handle_connection(&app_config, stream_addr);
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
}

impl AppConfig {
    fn from_config(config: Config) -> Result<Self, Box<dyn Error>> {
        let classifier = Classifier::new(&config).unwrap(); // FIXME
        let garbage = Garbage::new(&config).unwrap(); // FIXME

        let handler = match config.server.mode {
            ServerMode::Proxy => proxy,
            ServerMode::Preflight => preflight,
        };

        Ok(AppConfig {
            config,
            classifier: Arc::new(classifier),
            garbage: Arc::new(garbage),
            handler,
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
        }
    }

    fn listen_addr(&self) -> SocketAddr {
        self.config.server.listen
    }

    fn same_as(&self, other: &Config) -> bool {
        self.config == *other
    }

    async fn listen(&self) -> std::io::Result<TcpListener> {
        let listener = TcpListener::bind(self.listen_addr()).await?;
        log::info!("Listening on {}", self.listen_addr());
        Ok(listener)
    }
}

fn handle_connection(app_config: &AppConfig, (stream, addr): (TcpStream, SocketAddr)) {
    let io = TokioIo::new(stream);
    let app = app_config.to_service(addr.ip());
    tokio::task::spawn(async move {
        if let Err(e) = http1::Builder::new().serve_connection(io, app).await {
            println!("Failed to serve connection: {e}");
        }
    });
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
