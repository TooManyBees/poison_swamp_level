use crate::classifier::{Classification, Classifier, Decision};
use crate::garbage::Garbage;
use hyper::service::Service;
use hyper::{Request, Response, StatusCode, body::Incoming as IncomingBody};
use std::{net::IpAddr, pin::Pin, sync::Arc, time::Instant};

type BodyType = Response<String>;
type HandlerOutput<'r> = (Classification<'r>, BodyType);
type ServiceFuture = Pin<Box<dyn Future<Output = Result<BodyType, hyper::Error>> + Send>>;
pub type HandlerType = for<'r> fn(&App, &'r Request<IncomingBody>) -> HandlerOutput<'r>;

#[derive(Debug, Clone)]
pub struct App {
    pub client_ip: IpAddr,
    pub classifier: Arc<Classifier>,
    pub garbage: Arc<Garbage>,
    pub handler: HandlerType,
    pub status_code_valid: http::StatusCode,
    pub status_code_spam: http::StatusCode,
    pub logging: bool,
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
        // record_metrics(classification);
        Box::pin(async { Ok(resp) })
    }
}

pub fn proxy<'r>(app: &App, req: &'r Request<IncomingBody>) -> HandlerOutput<'r> {
    let classification = app.classifier.classify(&req);
    match classification.decision {
        Decision::Valid(_) => {
            let resp = empty_response(app.status_code_valid);
            (classification, resp)
        }
        Decision::Spam(_) => (classification, app.garbage_response(&req)),
    }
}

pub fn preflight<'r>(app: &App, req: &'r Request<IncomingBody>) -> HandlerOutput<'r> {
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
