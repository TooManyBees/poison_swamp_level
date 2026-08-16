mod robots_json;

use crate::config::Config;
use aho_corasick::BuildError;
use http::Request;
use http::header::{HeaderName, USER_AGENT};
use maxminddb::{MaxMindDbError, Reader, geoip2::Asn};
use robots_json::{RobotsJson, RobotsJsonError, load_robots_json};
use std::net::IpAddr;

#[derive(Debug)]
pub struct Classifier {
    poisons: Vec<String>,
    trusted_decision_header: Option<HeaderName>,

    asns_db: Option<maxminddb::Reader<Vec<u8>>>,
    unwanted_asns: Vec<u32>,
    robots_matcher: Option<RobotsJson>,
}

impl Classifier {
    pub fn new(config: &Config) -> Result<Self, ClassifierError> {
        let trusted_decision_header = config
            .classifier
            .trusted_decision_header
            .as_ref()
            .map(|header_name| {
                HeaderName::from_bytes(header_name.as_bytes())
                    .map_err(|_| ClassifierError::InvalidHeader(header_name.to_string()))
            })
            .transpose()?;

        let asns_db = config
            .classifier
            .asns_db_path
            .as_ref()
            .map(Reader::open_readfile)
            .transpose()?;

        let unwanted_asns = config.classifier.unwanted_asns.clone();

        if !unwanted_asns.is_empty() && asns_db.is_none() {
            // TODO: warn that asns can't be looked up
        }

        let robots_matcher = config
            .classifier
            .robots_json_path
            .as_ref()
            .map(load_robots_json)
            .transpose()?;

        Ok(Classifier {
            poisons: config.poisons.clone(),
            trusted_decision_header,

            asns_db,
            unwanted_asns,
            robots_matcher,
        })
    }

    pub fn trusted_decision<B>(&self, req: &Request<B>) -> Option<TrustedDecision> {
        self.trusted_decision_header
            .as_ref()
            .and_then(|header| req.headers().get(header))
            .and_then(|decision| match decision.as_ref() {
                b"valid" => Some(TrustedDecision::Valid),
                b"spam" => Some(TrustedDecision::Spam),
                other => None, // FIXME account for this
            })
    }

    fn asn<B>(&self, req: &Request<B>) -> Result<Option<u32>, MaxMindDbError> {
        let remote_ip = req.extensions().get::<IpAddr>().copied();
        if let Some((db, ip_addr)) = self.asns_db.as_ref().zip(remote_ip) {
            return Ok(db
                .lookup(ip_addr)?
                .decode::<Asn>()?
                .and_then(|asn| asn.autonomous_system_number));
        }
        Ok(None)
    }
}

#[derive(Debug)]
pub enum TrustedDecision {
    Valid,
    Spam,
}

#[derive(Debug)]
pub enum ClassifierError {
    Io(std::io::Error),
    InvalidHeader(String),
    MaxMindDb(MaxMindDbError),
    Json(serde_json::Error),
    Matcher(BuildError),
}

impl From<MaxMindDbError> for ClassifierError {
    fn from(e: MaxMindDbError) -> ClassifierError {
        match e {
            MaxMindDbError::Io(e) => ClassifierError::Io(e),
            e => ClassifierError::MaxMindDb(e),
        }
    }
}

impl From<RobotsJsonError> for ClassifierError {
    fn from(e: RobotsJsonError) -> ClassifierError {
        match e {
            RobotsJsonError::Io(e) => ClassifierError::Io(e),
            RobotsJsonError::Json(e) => ClassifierError::Json(e),
            RobotsJsonError::Matcher(e) => ClassifierError::Matcher(e),
        }
    }
}
