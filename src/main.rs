use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode, body::Incoming as IncomingBody};
use hyper_util::rt::TokioIo;
use poison_swamp_level::{Classifier, Config, Garbage};
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;

#[derive(Debug)]
struct App {
    classifier: Classifier,
    garbage: Garbage,
}

impl Service<Request<IncomingBody>> for App {
    type Response = Response<String>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, req: Request<IncomingBody>) -> Self::Future {
        let path = req.uri().path();
        let body = self.garbage.render(path);
        let res = Response::builder()
            .status(StatusCode::OK)
            .body(body)
            .unwrap();

        Box::pin(async { Ok(res) })
    }
}

#[tokio::main(flavor = "local")]
async fn main() {
    let config = Config::read_from_file("./config.json").unwrap();
    let classifier = Classifier::new(&config).unwrap();
    let garbage = Garbage::new(&config).unwrap();

    let app = Arc::new(App {
        classifier,
        garbage,
    });

    let listener = TcpListener::bind(config.server.interface).await.unwrap();

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let app_clone = app.clone();
        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, app_clone).await {
                println!("Failed to serve connection: {e}");
            }
        });
    }
}
