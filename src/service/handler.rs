use crate::classifier::{Classification, Classifier, Decision};
use crate::garbage::Garbage;
use crate::metrics::{Metrics, record_request};
use hyper::service::Service;
use hyper::{Request, Response, StatusCode, body::Incoming as IncomingBody};
use std::{
    net::IpAddr,
    pin::Pin,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Instant,
};

type BodyType = Response<String>;
type HandlerOutput<'r> = (Classification<'r>, BodyType);
type ServiceFuture = Pin<Box<dyn Future<Output = Result<BodyType, hyper::Error>> + Send>>;
pub type HandlerType = for<'r> fn(&PslHandler, &'r Request<IncomingBody>) -> HandlerOutput<'r>;

#[derive(Debug, Clone)]
pub struct PslHandler {
    pub client_ip: Option<IpAddr>,
    pub classifier: Arc<Classifier>,
    pub garbage: Arc<Garbage>,
    pub handler: HandlerType,
    pub status_code_valid: http::StatusCode,
    pub status_code_spam: http::StatusCode,
    pub logging: bool,
    pub metrics: Arc<Mutex<Metrics>>,
}

impl PslHandler {
    fn garbage_response<B>(&self, req: &Request<B>) -> Response<String> {
        let path = request_path(&req);
        let body = self.garbage.render(path);
        Response::builder()
            .status(StatusCode::OK)
            .body(body)
            .unwrap()
    }
}

impl Service<Request<IncomingBody>> for PslHandler {
    type Response = BodyType;
    type Error = hyper::Error;
    type Future = ServiceFuture;

    fn call(&self, mut req: Request<IncomingBody>) -> Self::Future {
        let client_ip = req
            .headers()
            .get("x-forwarded-for")
            // At some point we should be able to IpAddr::parse_ascii or something
            // https://github.com/rust-lang/rust/issues/101035
            .and_then(|v| v.to_str().ok())
            .and_then(|v| IpAddr::from_str(v).ok())
            .or(self.client_ip);
        req.extensions_mut().insert(client_ip);
        let now = Instant::now();
        let (classification, resp) = (self.handler)(self, &req);
        let elapsed = now.elapsed().as_millis();
        if self.logging {
            log::info!(
                host = classification.host,
                path = request_path(&req),
                status = resp.status().as_u16(),
                elapsed_ms = elapsed,
                client_ip = classification.remote_ip,
                asn = classification.asn,
                poison = classification.poison,
                user_agent = classification.agent;
                "Response {} {} {}ms {}",
                request_path(&req),
                resp.status().as_u16(),
                elapsed,
                classification.decision,
            );
        }
        // TODO: don't do this if metrics are disabled
        record_request(&self.metrics, classification);
        Box::pin(async { Ok(resp) })
    }
}

pub fn proxy<'r>(app: &PslHandler, req: &'r Request<IncomingBody>) -> HandlerOutput<'r> {
    let classification = app.classifier.classify(&req);
    match classification.decision {
        Decision::Valid(_) => {
            let resp = empty_response(app.status_code_valid);
            (classification, resp)
        }
        Decision::Spam(_) => (classification, app.garbage_response(&req)),
    }
}

pub fn preflight<'r>(app: &PslHandler, req: &'r Request<IncomingBody>) -> HandlerOutput<'r> {
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

fn empty_response(status_code: StatusCode) -> BodyType {
    Response::builder()
        .status(status_code)
        .body("".into())
        .unwrap()
}

fn request_path<B>(req: &Request<B>) -> &str {
    req.uri()
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/")
}

#[derive(Debug, Clone)]
pub struct MetricsHandler {
    pub metrics: Arc<Mutex<Metrics>>,
}

impl Service<Request<IncomingBody>> for MetricsHandler {
    type Response = BodyType;
    type Error = hyper::Error;
    type Future = ServiceFuture;

    fn call(&self, _req: Request<IncomingBody>) -> Self::Future {
        let output = {
            let mut metrics = self.metrics.lock().unwrap();
            metrics.to_prometheus()
        };

        let resp = Response::builder()
            .status(StatusCode::OK)
            .body(output)
            .unwrap();

        Box::pin(async { Ok(resp) })
    }
}
