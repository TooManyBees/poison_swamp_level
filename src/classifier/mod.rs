use crate::config::Config;
use http::Request;
use http::header::HeaderName;

#[derive(Debug)]
pub struct Classifier {
    poisons: Vec<String>,
    trusted_decision_header: Option<HeaderName>,
}

impl Classifier {
    pub fn new(config: &Config) -> Result<Self, ClassifierError> {
        let trusted_decision_header = match &config.classifier.trusted_decision_header {
            Some(header_name) => match HeaderName::from_bytes(header_name.as_bytes()) {
                Ok(header) => Some(header),
                Err(_) => {
                    return Err(ClassifierError::InvalidHeader(header_name.clone()));
                }
            },
            None => None,
        };
        Ok(Classifier {
            poisons: config.poisons.clone(),
            trusted_decision_header,
        })
    }

    pub fn trusted_decision<B>(&self, req: &Request<B>) -> Option<TrustedDecision> {
        self.trusted_decision_header.as_ref()
            .and_then(|header| req.headers().get(header))
            .and_then(|decision| match decision.as_ref() {
                b"valid" => Some(TrustedDecision::Valid),
                b"spam" => Some(TrustedDecision::Spam),
                other => None, // FIXME account for this
            })
    }
}

#[derive(Debug)]
pub enum TrustedDecision {
    Valid,
    Spam,
}

#[derive(Debug)]
pub enum ClassifierError {
    InvalidHeader(String),
}
