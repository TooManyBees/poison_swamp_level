use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use poison_swamp_level::handler::{App, preflight, proxy};
use poison_swamp_level::{Classifier, Config, Garbage, ServerMode};
use std::sync::Arc;
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

    let classifier = Classifier::new(&config).unwrap();
    let garbage = Garbage::new(&config).unwrap();

    let classifier = Arc::new(classifier);
    let garbage = Arc::new(garbage);

    let listener = TcpListener::bind(config.server.listen).await.unwrap();

    let handler = match config.server.mode {
        ServerMode::Proxy => proxy,
        ServerMode::Preflight => preflight,
    };

    log::info!("Listening on {}", config.server.listen);

    loop {
        let (stream, addr) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let app = App {
            client_ip: addr.ip(),
            classifier: classifier.clone(),
            garbage: garbage.clone(),
            handler,
            status_code_valid: config.server.status_code_valid,
            status_code_spam: config.server.status_code_spam,
            logging: config.logging.request_handler,
        };
        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, app).await {
                println!("Failed to serve connection: {e}");
            }
        });
    }
}
