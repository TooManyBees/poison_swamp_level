use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode, body::Incoming as IncomingBody};
use hyper_util::rt::TokioIo;
use poison_swamp_level::{Classifier, Config, Garbage};
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
struct App {
    client_ip: IpAddr,
    classifier: Arc<Classifier>,
    garbage: Arc<Garbage>,
}

impl Service<Request<IncomingBody>> for App {
    type Response = Response<String>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, mut req: Request<IncomingBody>) -> Self::Future {
        req.extensions_mut().insert(self.client_ip);

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

    let classifier = Arc::new(classifier);
    let garbage = Arc::new(garbage);

    let listener = TcpListener::bind(config.server.listen).await.unwrap();

    loop {
        let (stream, addr) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let app = App {
            client_ip: addr.ip(),
            classifier: classifier.clone(),
            garbage: garbage.clone(),
        };
        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, app).await {
                println!("Failed to serve connection: {e}");
            }
        });
    }
}
