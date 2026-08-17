use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode, body::Incoming as IncomingBody};
use hyper_util::rt::TokioIo;
use poison_swamp_level::{Classification, Classifier, Config, Garbage, TrustedDecision};
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
struct App {
    client_ip: IpAddr,
    classifier: Arc<Classifier>,
    garbage: Arc<Garbage>,
    status_code_valid: http::StatusCode,
    status_code_spam: http::StatusCode,
}

fn preflight_check(
    app: &App,
    req: Request<IncomingBody>,
) -> Pin<Box<dyn Future<Output = Result<Response<String>, hyper::Error>> + Send>> {
    if let Some(TrustedDecision::Spam) = app.classifier.trusted_decision(&req) {
        let path = req.uri().path();
        let body = app.garbage.render(path);
        let res = Response::builder()
            .status(StatusCode::OK)
            .body(body)
            .unwrap();
        return Box::pin(async { Ok(res) });
    }

    let preflight_status = match app.classifier.classify(&req) {
        Classification::Valid(_) => app.status_code_valid,
        Classification::Spam(_) => app.status_code_spam,
    };

    let res = Response::builder()
        .status(preflight_status)
        .body("".into())
        .unwrap();
    Box::pin(async { Ok(res) })
}

impl Service<Request<IncomingBody>> for App {
    type Response = Response<String>;
    type Error = hyper::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn call(&self, mut req: Request<IncomingBody>) -> Self::Future {
        req.extensions_mut().insert(self.client_ip);
        preflight_check(self, req)
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
            status_code_valid: config.server.status_code_valid,
            status_code_spam: config.server.status_code_spam,
        };
        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, app).await {
                println!("Failed to serve connection: {e}");
            }
        });
    }
}
