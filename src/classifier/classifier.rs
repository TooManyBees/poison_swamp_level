use super::matcher::Matcher;
use super::robots_json::{RobotsJsonError, load_robots_json};
use crate::config::Config;
use aho_corasick::BuildError;
use http::{
    Request,
    header::{HeaderName, USER_AGENT},
};
use maxminddb::{MaxMindDbError, Reader, geoip2::Asn};
use std::fmt;
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

    pub fn trusted_decision<'r, B>(&self, req: &'r Request<B>) -> Option<Classification<'r>> {
        self.trusted_decision_header
            .as_ref()
            .and_then(|header_name| req.headers().get(header_name))
            .and_then(|header_value| match header_value.as_ref() {
                b"valid" => Some(Decision::Valid(ValidReason::TrustedDecision)),
                b"spam" => Some(Decision::Spam(SpamReason::TrustedDecision)),
                _other => None, // FIXME account for this
            })
            .map(|decision| {
                let mut classification = Classification::default();
                classification.decision = decision;
                classification
            })
    }

    fn trusted_path<'r, B>(&self, req: &'r Request<B>) -> Option<&'r str> {
        let path = req.uri().path();
        if self.trusted_paths.iter().any(|p| p == path) {
            return Some(path);
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

    fn lookup_asn(
        db: &maxminddb::Reader<Vec<u8>>,
        ip: IpAddr,
    ) -> Result<Option<u32>, MaxMindDbError> {
        Ok(db
            .lookup(ip)?
            .decode::<Asn>()?
            .and_then(|asn| asn.autonomous_system_number))
    }

    fn asn<B>(&self, req: &Request<B>) -> Option<u32> {
        let db = self.asns_db.as_ref()?;
        let remote_ip = req.extensions().get::<IpAddr>().copied()?;
        match Classifier::lookup_asn(db, remote_ip) {
            Ok(Some(asn)) => Some(asn),
            Ok(_) => None,
            Err(e) => {
                log::error!("Error looking up ASN: {}", e);
                None
            }
        }
    }

    fn trusted_agent<'r>(&self, req: Option<&'r str>) -> Option<&'r str> {
        match_agent(self.trusted_agents.as_ref(), req)
    }

    fn unwanted_agent<'r>(&self, req: Option<&'r str>) -> Option<&'r str> {
        match_agent(self.unwanted_agents.as_ref(), req)
    }

    pub fn classify<'r, B>(&self, req: &'r Request<B>) -> Classification<'r> {
        let mut info = Classification {
            ip: req.extensions().get::<IpAddr>().copied(),
            poison: self.poisoned_path(req),
            asn: self.asn(req),
            agent: req.headers().get(USER_AGENT).and_then(|h| h.to_str().ok()),
            decision: Decision::Valid(ValidReason::Default),
        };

        if let Some(path) = self.trusted_path(req) {
            info.decision = Decision::Valid(ValidReason::TrustedPath(path));
            return info;
        }

        if let Some(ip) = info.ip.filter(|ip| self.trusted_ips.contains(&ip)) {
            info.decision = Decision::Valid(ValidReason::TrustedIP(ip));
            return info;
        }

        if let Some(agent) = self.trusted_agent(info.agent) {
            info.decision = Decision::Valid(ValidReason::TrustedAgent(agent));
            return info;
        }

        if let Some(poison) = info.poison {
            info.decision = Decision::Spam(SpamReason::Poison(poison));
            return info;
        }

        if let Some(agent) = self.unwanted_agent(info.agent) {
            info.decision = Decision::Spam(SpamReason::UnwantedAgent(agent));
            return info;
        }

        if let Some(asn) = info.asn.filter(|asn| self.unwanted_asns.contains(&asn)) {
            info.decision = Decision::Spam(SpamReason::UnwantedASN(asn));
            return info;
        }

        info
    }
}

fn match_agent<'r>(matcher: Option<&Matcher>, header_value: Option<&'r str>) -> Option<&'r str> {
    if let Some((matcher, user_agent)) = matcher.zip(header_value) {
        if let Some(m) = matcher.find(user_agent) {
            return Some(&user_agent[m.range()]);
        }
    }
    None
}

#[derive(Debug, Default)]
pub struct Classification<'a> {
    pub ip: Option<IpAddr>,
    pub agent: Option<&'a str>,
    pub asn: Option<u32>,
    pub poison: Option<&'a str>,
    pub decision: Decision<'a>,
}

#[derive(Debug)]
pub enum Decision<'a> {
    Valid(ValidReason<'a>),
    Spam(SpamReason<'a>),
}

impl<'a> Default for Decision<'a> {
    fn default() -> Self {
        Decision::Valid(ValidReason::Default)
    }
}

impl<'a> fmt::Display for Decision<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Decision::Valid(r) => write!(f, "valid {r}"),
            Decision::Spam(r) => write!(f, "spam {r}"),
        }
    }
}

#[derive(Debug)]
pub enum ValidReason<'a> {
    Default,
    TrustedIP(IpAddr),
    TrustedPath(&'a str),
    TrustedAgent(&'a str),
    TrustedDecision,
}

#[derive(Debug)]
pub enum SpamReason<'a> {
    Poison(&'a str),
    UnwantedASN(u32),
    UnwantedAgent(&'a str),
    TrustedDecision,
}

impl<'a> fmt::Display for ValidReason<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ValidReason::Default => f.write_str("default"),
            ValidReason::TrustedIP(ip) => write!(f, "trusted IP {ip}"),
            ValidReason::TrustedPath(path) => write!(f, "trusted path {path:?}"),
            ValidReason::TrustedAgent(agent) => write!(f, "trusted agent {agent:?}"),
            ValidReason::TrustedDecision => f.write_str("trusted decision header"),
        }
    }
}

impl<'a> fmt::Display for SpamReason<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SpamReason::Poison(p) => write!(f, "poison {p:?}"),
            SpamReason::UnwantedASN(asn) => write!(f, "unwanted ASN {asn}"),
            SpamReason::UnwantedAgent(agent) => write!(f, "unwanted agent {agent:?}"),
            SpamReason::TrustedDecision => f.write_str("trusted decision header"),
        }
    }
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

impl fmt::Display for ClassifierError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClassifierError::Io(e) => e.fmt(f),
            ClassifierError::InvalidHeader(s) => write!(f, "invalid header: {s}"),
            ClassifierError::MaxMindDb(e) => e.fmt(f),
            ClassifierError::Json(e) => e.fmt(f),
            ClassifierError::Matcher(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ClassifierError {}
