use super::matcher::Matcher;
use super::robots_json::{RobotsJsonError, load_robots_json};
use crate::config::Config;
use aho_corasick::BuildError;
use http::Request;
use http::header::{HeaderName, USER_AGENT};
use maxminddb::{MaxMindDbError, Reader, geoip2::Asn};
use std::net::IpAddr;

#[derive(Debug)]
pub struct Classifier {
    poisons: Vec<String>,
    trusted_decision_header: Option<HeaderName>,

    asns_db: Option<maxminddb::Reader<Vec<u8>>>,
    unwanted_asns: Vec<u32>,
    robots_matcher: Option<Matcher>,
    // trusted_ips: Vec<String>, // FIXME
    // trusted_paths: Vec<String>,
    // trusted_agents: Vec<String>,
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
            // trusted_ips: vec![],
            // trusted_paths: vec![],
            // trusted_agents: vec![],
        })
    }

    pub fn trusted_decision<B>(&self, req: &Request<B>) -> Option<TrustedDecision> {
        self.trusted_decision_header
            .as_ref()
            .and_then(|header| req.headers().get(header))
            .and_then(|decision| match decision.as_ref() {
                b"valid" => Some(TrustedDecision::Valid),
                b"spam" => Some(TrustedDecision::Spam),
                _other => None, // FIXME account for this
            })
    }

    fn poisoned_path<'r, B>(&self, req: &'r Request<B>) -> Option<&'r str> {
        let path = req.uri().path();
        for poison in &self.poisons {
            if let Some(idx) = path.find(poison) {
                return Some(&path[idx..idx + poison.len()]);
            }
        }
        None
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

    fn unwanted_asn<B>(&self, req: &Request<B>) -> Option<u32> {
        match self.asn(req) {
            Ok(Some(asn)) if self.unwanted_asns.contains(&asn) => Some(asn),
            Ok(_) => None,
            Err(_e) => {
                // TODO log an error
                None
            }
        }
    }

    fn unwanted_agent<'r, B>(&self, req: &'r Request<B>) -> Option<&'r str> {
        if let Some((matcher, user_agent)) = self
            .robots_matcher
            .as_ref()
            .zip(req.headers().get(USER_AGENT).and_then(|h| h.to_str().ok()))
        {
            if let Some(m) = matcher.find(user_agent) {
                return Some(&user_agent[m.range()]);
            }
        }
        None
    }

    pub fn classify<B>(&self, req: &Request<B>) -> Classification {
        if let Some(_poison) = self.poisoned_path(req) {
            return Classification::Spam(SpamReason::Poison);
        }

        if let Some(_agent) = self.unwanted_agent(req) {
            return Classification::Spam(SpamReason::UnwantedAgent);
        }

        if let Some(_asn) = self.unwanted_asn(req) {
            return Classification::Spam(SpamReason::UnwantedASN);
        }

        Classification::Valid(ValidReason::Default)
    }
}

#[derive(Debug, Copy, Clone)]
pub enum TrustedDecision {
    Valid,
    Spam,
}

#[derive(Debug)]
pub enum Classification {
    Valid(ValidReason),
    Spam(SpamReason),
}

#[derive(Debug)]
pub enum ValidReason {
    Default,
    TrustedIP,
    TrustedPath,
    TrustedAgent,
}

#[derive(Debug)]
pub enum SpamReason {
    Poison,
    UnwantedASN,
    UnwantedAgent,
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
