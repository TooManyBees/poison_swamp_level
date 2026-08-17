use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode, body::Incoming as IncomingBody};
use hyper_util::rt::TokioIo;
use poison_swamp_level::{
    Classification, Classifier, Config, Garbage, ServerMode, TrustedDecision,
};
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;

type ServiceFuture = Pin<Box<dyn Future<Output = Result<Response<String>, hyper::Error>> + Send>>;

#[derive(Debug, Clone)]
struct App {
    client_ip: IpAddr,
    classifier: Arc<Classifier>,
    garbage: Arc<Garbage>,
    handler: fn(&App, Request<IncomingBody>) -> ServiceFuture,
    status_code_valid: http::StatusCode,
    status_code_spam: http::StatusCode,
}

impl App {
    fn garbage_response<B>(&self, req: &Request<B>) -> Response<String> {
        let path = req
            .uri()
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or(req.uri().path());

        let body = self.garbage.render(path);
        Response::builder()
            .status(StatusCode::OK)
            .body(body)
            .unwrap()
    }
}

impl Service<Request<IncomingBody>> for App {
    type Response = Response<String>;
    type Error = hyper::Error;
    type Future = ServiceFuture;

    fn call(&self, mut req: Request<IncomingBody>) -> Self::Future {
        req.extensions_mut().insert(self.client_ip);
        (self.handler)(self, req)
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

    let handler = match config.server.mode {
        ServerMode::Proxy => server_proxy,
        ServerMode::Preflight => server_preflight,
    };

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
        };
        tokio::task::spawn(async move {
            if let Err(e) = http1::Builder::new().serve_connection(io, app).await {
                println!("Failed to serve connection: {e}");
            }
        });
    }
}

fn server_proxy(app: &App, req: Request<IncomingBody>) -> ServiceFuture {
    let resp = match app.classifier.classify(&req) {
        Classification::Valid(_) => Response::builder()
            .status(app.status_code_valid)
            .body("".into())
            .unwrap(),
        Classification::Spam(_) => app.garbage_response(&req),
    };

    Box::pin(async { Ok(resp) })
}

fn server_preflight(app: &App, req: Request<IncomingBody>) -> ServiceFuture {
    if let Some(TrustedDecision::Spam) = app.classifier.trusted_decision(&req) {
        let resp = app.garbage_response(&req);
        return Box::pin(async { Ok(resp) });
    }

    let preflight_status = match app.classifier.classify(&req) {
        Classification::Valid(_) => app.status_code_valid,
        Classification::Spam(_) => app.status_code_spam,
    };

    let resp = Response::builder()
        .status(preflight_status)
        .body("".into())
        .unwrap();
    Box::pin(async { Ok(resp) })
}
