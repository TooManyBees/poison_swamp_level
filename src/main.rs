use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode, body::Incoming as IncomingBody};
use hyper_util::rt::TokioIo;
use poison_swamp_level::{Classification, Classifier, Config, Garbage, ServerMode};
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;

type BodyType = Response<String>;
type HandlerOutput<'r> = (Classification<'r>, BodyType);
type ServiceFuture = Pin<Box<dyn Future<Output = Result<BodyType, hyper::Error>> + Send>>;

#[derive(Debug, Clone)]
struct App {
    client_ip: IpAddr,
    classifier: Arc<Classifier>,
    garbage: Arc<Garbage>,
    handler: for<'r> fn(&App, &'r Request<IncomingBody>) -> HandlerOutput<'r>,
    status_code_valid: http::StatusCode,
    status_code_spam: http::StatusCode,
    logging: bool,
}

impl App {
    fn garbage_response<B>(&self, req: &Request<B>) -> Response<String> {
        let path = request_path(&req);
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
        let (decision, resp) = (self.handler)(self, &req);
        if self.logging {
            log::info!(
                "Response {} {:?} decision {}",
                resp.status().as_str(),
                request_path(&req),
                decision,
            );
        }
        Box::pin(async { Ok(resp) })
    }
}

#[tokio::main(flavor = "local")]
async fn main() {
    let config = Config::read_from_file("./config.kdl").unwrap();

    env_logger::builder()
        .filter(None, config.logging.level)
        .init();

    let classifier = Classifier::new(&config).unwrap();
    let garbage = Garbage::new(&config).unwrap();

    let classifier = Arc::new(classifier);
    let garbage = Arc::new(garbage);

    let listener = TcpListener::bind(config.server.listen).await.unwrap();

    let handler = match config.server.mode {
        ServerMode::Proxy => server_proxy,
        ServerMode::Preflight => server_preflight,
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

fn server_proxy<'r>(app: &App, req: &'r Request<IncomingBody>) -> HandlerOutput<'r> {
    match app.classifier.classify(&req) {
        decision @ Classification::Valid(_) => {
            let resp = Response::builder()
                .status(app.status_code_valid)
                .body("".into())
                .unwrap();
            (decision, resp)
        }
        decision @ Classification::Spam(_) => (decision, app.garbage_response(&req)),
    }
}

fn server_preflight<'r>(app: &App, req: &'r Request<IncomingBody>) -> HandlerOutput<'r> {
    if let Some(d @ Classification::Valid(_)) = app.classifier.trusted_decision(&req) {
        let resp = app.garbage_response(&req);
        return (d, resp);
    }

    let (decision, preflight_status) = match app.classifier.classify(&req) {
        d @ Classification::Valid(_) => (d, app.status_code_valid),
        d @ Classification::Spam(_) => (d, app.status_code_spam),
    };

    let resp = Response::builder()
        .status(preflight_status)
        .body("".into())
        .unwrap();

    (decision, resp)
}

fn request_path<B>(req: &Request<B>) -> &str {
    req.uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or(req.uri().path())
}
