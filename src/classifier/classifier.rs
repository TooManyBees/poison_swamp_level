use super::matcher::Matcher;
use super::robots_json::{RobotsJsonError, load_robots_json};
use crate::config::Config;
use aho_corasick::BuildError;
use http::{
    Request,
    header::{HeaderName, USER_AGENT},
};
use maxminddb::{MaxMindDbError, Reader, geoip2::Asn};
use std::net::IpAddr;
use std::time::Instant;

#[derive(Debug)]
pub struct Classifier {
    poisons: Vec<String>,
    trusted_decision_header: Option<HeaderName>,

    asns_db: Option<maxminddb::Reader<Vec<u8>>>,
    unwanted_asns: Vec<u32>,
    unwanted_agents: Option<Matcher>,

    trusted_paths: Vec<String>,
    trusted_ips: Vec<IpAddr>,
    trusted_agents: Option<Matcher>,
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

        let asns_db = if let Some(path) = config.classifier.asns_db_path.as_ref() {
            let then = Instant::now();
            let db = Reader::open_readfile(path)?;
            log::debug!("Read ANSs database in {}ms", then.elapsed().as_millis());
            Some(db)
        } else {
            None
        };

        let unwanted_asns = config.classifier.unwanted_asns.clone();

        if !unwanted_asns.is_empty() && asns_db.is_none() {
            log::warn!("ASNs will not be detected because ANS database was not provided");
        }

        let robots_json = config
            .classifier
            .robots_json_path
            .as_ref()
            .map(load_robots_json)
            .transpose()?;

        let mut unwanted_agents = config.classifier.unwanted_agents.clone();
        if let Some(robots) = robots_json {
            unwanted_agents.extend_from_slice(&robots);
        }

        let unwanted_agents = if !unwanted_agents.is_empty() {
            let then = Instant::now();
            let matcher = Matcher::new(unwanted_agents)?;
            log::debug!(
                "Created unwanted agents matcher in {}ms",
                then.elapsed().as_millis()
            );
            Some(matcher)
        } else {
            None
        };

        let trusted_agents = if !config.classifier.trusted_agents.is_empty() {
            let then = Instant::now();
            let matcher = Matcher::new(config.classifier.trusted_agents.clone())?;
            log::debug!(
                "Created trusted agents matcher in {}ms",
                then.elapsed().as_millis()
            );
            Some(matcher)
        } else {
            None
        };

        Ok(Classifier {
            poisons: config.garbage.poisons.clone(),
            trusted_decision_header,

            asns_db,
            unwanted_asns,
            unwanted_agents,
            trusted_paths: config.classifier.trusted_paths.clone(),
            trusted_ips: config.classifier.trusted_ips.clone(),
            trusted_agents,
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

    fn trusted_path<'r, B>(&self, req: &'r Request<B>) -> Option<&'r str> {
        let path = req.uri().path();
        if self.trusted_paths.iter().any(|p| p == path) {
            return Some(path);
        }
        None
    }

    fn trusted_ip<B>(&self, req: &Request<B>) -> Option<IpAddr> {
        if let Some(remote_ip) = req.extensions().get::<IpAddr>().copied() {
            if self.trusted_ips.contains(&remote_ip) {
                return Some(remote_ip);
            }
        }
        None
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
            Err(e) => {
                log::error!("Error looking up ASN: {}", e);
                None
            }
        }
    }

    fn trusted_agent<'r, B>(&self, req: &'r Request<B>) -> Option<&'r str> {
        match_agent(self.trusted_agents.as_ref(), req)
    }

    fn unwanted_agent<'r, B>(&self, req: &'r Request<B>) -> Option<&'r str> {
        match_agent(self.unwanted_agents.as_ref(), req)
    }

    pub fn classify<'r, B>(&self, req: &'r Request<B>) -> Classification<'r> {
        if let Some(path) = self.trusted_path(req) {
            return Classification::Valid(ValidReason::TrustedPath(path));
        }

        if let Some(ip) = self.trusted_ip(req) {
            return Classification::Valid(ValidReason::TrustedIP(ip));
        }

        if let Some(agent) = self.trusted_agent(req) {
            return Classification::Valid(ValidReason::TrustedAgent(agent));
        }

        if let Some(poison) = self.poisoned_path(req) {
            return Classification::Spam(SpamReason::Poison(poison));
        }

        if let Some(agent) = self.unwanted_agent(req) {
            return Classification::Spam(SpamReason::UnwantedAgent(agent));
        }

        if let Some(asn) = self.unwanted_asn(req) {
            return Classification::Spam(SpamReason::UnwantedASN(asn));
        }

        Classification::Valid(ValidReason::Default)
    }
}

fn match_agent<'r, B>(matcher: Option<&Matcher>, req: &'r Request<B>) -> Option<&'r str> {
    let header_value = req.headers().get(USER_AGENT).and_then(|h| h.to_str().ok());
    if let Some((matcher, user_agent)) = matcher.zip(header_value) {
        if let Some(m) = matcher.find(user_agent) {
            return Some(&user_agent[m.range()]);
        }
    }
    None
}

#[derive(Debug, Copy, Clone)]
pub enum TrustedDecision {
    Valid,
    Spam,
}

#[derive(Debug)]
pub enum Classification<'a> {
    Valid(ValidReason<'a>),
    Spam(SpamReason<'a>),
}

#[derive(Debug)]
pub enum ValidReason<'a> {
    Default,
    TrustedIP(IpAddr),
    TrustedPath(&'a str),
    TrustedAgent(&'a str),
}

#[derive(Debug)]
pub enum SpamReason<'a> {
    Poison(&'a str),
    UnwantedASN(u32),
    UnwantedAgent(&'a str),
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
        }
    }
}

impl From<BuildError> for ClassifierError {
    fn from(e: BuildError) -> ClassifierError {
        ClassifierError::Matcher(e)
    }
}
