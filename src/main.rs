use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode, body::Incoming as IncomingBody};
use hyper_util::rt::TokioIo;
use poison_swamp_level::{Classification, Classifier, Config, Decision, Garbage, ServerMode};
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
        let (classification, resp) = (self.handler)(self, &req);
        if self.logging {
            log::info!(
                "Response {} {:?} decision {}",
                resp.status().as_str(),
                request_path(&req),
                classification.decision,
            );
        }
        Box::pin(async { Ok(resp) })
    }
}

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

fn empty_response(status_code: StatusCode) -> BodyType {
    Response::builder()
        .status(status_code)
        .body("".into())
        .unwrap()
}

fn server_proxy<'r>(app: &App, req: &'r Request<IncomingBody>) -> HandlerOutput<'r> {
    let classification = app.classifier.classify(&req);
    match classification.decision {
        Decision::Valid(_) => {
            let resp = empty_response(app.status_code_valid);
            (classification, resp)
        }
        Decision::Spam(_) => (classification, app.garbage_response(&req)),
    }
}

fn server_preflight<'r>(app: &App, req: &'r Request<IncomingBody>) -> HandlerOutput<'r> {
    if let Some(classification) = app.classifier.trusted_decision(&req) {
        if let Decision::Spam(_) = classification.decision {
            let resp = app.garbage_response(&req);
            return (classification, resp);
        }
    }

    let classification = app.classifier.classify(&req);
    let preflight_status = match classification.decision {
        Decision::Valid(_) => app.status_code_valid,
        Decision::Spam(_) => app.status_code_spam,
    };

    let resp = empty_response(preflight_status);
    (classification, resp)
}

fn request_path<B>(req: &Request<B>) -> &str {
    req.uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
}
