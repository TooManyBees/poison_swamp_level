use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use poison_swamp_level::handler::{App, HandlerType, preflight, proxy};
use poison_swamp_level::{Classifier, Config, Garbage, ServerMode};
use std::{
    error::Error,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::net::TcpListener;

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

    // TODO: signal handler for reloading config (except log level)
    // on signal, background thread to load the config and create the
    // generator/classifier
    // tokio::select! to loop between listener, signal handler, config reload result
    // if config.server.listen changes, break out of entire loop and restart it

    let app_config = AppConfig::from_config(config).unwrap();

    let listener = app_config.listen().await.unwrap();

    loop {
        let (stream, addr) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let app = app_config.to_service(addr.ip());
        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, app).await {
                println!("Failed to serve connection: {e}");
            }
        });
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

    async fn listen(&self) -> std::io::Result<TcpListener> {
        let listener = TcpListener::bind(self.listen_addr()).await?;
        log::info!("Listening on {}", self.listen_addr());
        Ok(listener)
    }
}
