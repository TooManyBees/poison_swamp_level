mod format;
mod persistence;
#[cfg(target_os = "linux")]
mod procfs;
mod structures;

use crate::classifier::{Classification, Decision, SpamReason, ValidReason};
use crate::config::Config;
use compact_str::{CompactString, ToCompactString};
use format::append_metric;
use persistence::{apply_persisted_metrics, load_persisted_metrics, write_persisted_metrics};
#[cfg(target_os = "linux")]
use procfs::append_procfs_metrics;
use std::mem::take;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use structures::{MetricKey, MetricLabel, NamedHashMap};

#[derive(Debug)]
pub struct Metrics {
    request_counter: NamedHashMap,
    classification_spam_counter: NamedHashMap,
    classification_valid_counter: NamedHashMap,
    asn_known_counter: NamedHashMap,
    asn_hidden_counter: NamedHashMap,

    persist_path: Option<String>,
    output_buffer: String,
}

impl Metrics {
    pub fn increment_request(&mut self, labels: Vec<MetricLabel>) {
        let key = MetricKey {
            name: self.request_counter.name.clone(),
            labels,
        };
        self.request_counter
            .entry(key)
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::Release);
    }

    pub fn increment_classification_spam(&mut self, labels: Vec<MetricLabel>) {
        let key = MetricKey {
            name: self.classification_spam_counter.name.clone(),
            labels,
        };
        self.classification_spam_counter
            .entry(key)
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::Release);
    }

    pub fn increment_classification_valid(&mut self, labels: Vec<MetricLabel>) {
        let key = MetricKey {
            name: self.classification_valid_counter.name.clone(),
            labels,
        };
        self.classification_valid_counter
            .entry(key)
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::Release);
    }

    pub fn increment_asn_known(&mut self, asn: u32) {
        let label = MetricLabel(CompactString::const_new("asn"), asn.to_compact_string());
        let key = MetricKey {
            name: self.asn_known_counter.name.clone(),
            labels: vec![label],
        };
        self.asn_known_counter
            .entry(key)
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::Release);
    }

    pub fn increment_asn_hidden(&mut self, asn: u32) {
        let label = MetricLabel(CompactString::const_new("asn"), asn.to_compact_string());
        let key = MetricKey {
            name: self.asn_hidden_counter.name.clone(),
            labels: vec![label],
        };
        self.asn_hidden_counter
            .entry(key)
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::Release);
    }

    pub fn to_prometheus(&mut self) -> String {
        let mut output_buffer = take(&mut self.output_buffer);
        output_buffer.clear();

        append_metric(&mut output_buffer, &self.request_counter);
        append_metric(&mut output_buffer, &self.classification_spam_counter);
        append_metric(&mut output_buffer, &self.classification_valid_counter);
        append_metric(&mut output_buffer, &self.asn_known_counter);
        append_metric(&mut output_buffer, &self.asn_hidden_counter);

        #[cfg(target_os = "linux")]
        append_procfs_metrics(&mut output_buffer);

        self.output_buffer = output_buffer;
        self.output_buffer.clone()
    }
}

pub fn init(config: &Config) -> Arc<Mutex<Metrics>> {
    let mut metrics = Metrics {
        request_counter: NamedHashMap::new("requests", "Number of requests"),
        classification_spam_counter: NamedHashMap::new(
            "classifications_spam",
            "Number of spam classifications",
        ),
        classification_valid_counter: NamedHashMap::new(
            "classifications_valid",
            "Number of valid classifications",
        ),
        asn_known_counter: NamedHashMap::new("asns_known", "ASNs whence originate spam"),
        asn_hidden_counter: NamedHashMap::new("asns_hidden", "ASNs hiding spam"),
        persist_path: None,
        output_buffer: String::new(),
    };

    if let Some(path) = config
        .metrics
        .as_ref()
        .and_then(|metrics| metrics.persist_path.as_ref())
    {
        metrics.persist_path = Some(path.clone());

        match load_persisted_metrics(path) {
            Ok(Some(p)) => {
                apply_persisted_metrics(&mut metrics, p);
                log::debug!("Loaded persisted metrics at {path}");
            }
            Ok(None) => log::debug!("No persisted metrics to load at {path}"),
            Err(e) => {
                log::error!("Couldn't load persisted metrics from {path}: {e}");
            }
        }
    }

    Arc::new(Mutex::new(metrics))
}

pub fn record_request(metrics: &Mutex<Metrics>, c: Classification) {
    let mut labels = Vec::with_capacity(1);
    if let Some(host) = c.host {
        labels.push(MetricLabel(
            CompactString::const_new("host"),
            CompactString::from(host),
        ));
    }
    let mut metrics = metrics.lock().unwrap();
    metrics.increment_request(labels.clone());
    match c.decision {
        Decision::Valid(reason) => match reason {
            ValidReason::Default => {
                labels.push(MetricLabel(
                    CompactString::const_new("reason"),
                    CompactString::const_new("default"),
                ));
                metrics.increment_classification_valid(labels);
            }
            ValidReason::TrustedIP(_) => {
                labels.push(MetricLabel(
                    CompactString::const_new("reason"),
                    CompactString::const_new("trusted ip"),
                ));
                metrics.increment_classification_valid(labels);
            }
            ValidReason::TrustedPath(_) => {
                labels.push(MetricLabel(
                    CompactString::const_new("reason"),
                    CompactString::const_new("trusted path"),
                ));
                metrics.increment_classification_valid(labels);
            }
            ValidReason::TrustedAgent(_) => {
                labels.push(MetricLabel(
                    CompactString::const_new("reason"),
                    CompactString::const_new("trusted agent"),
                ));
                metrics.increment_classification_valid(labels);
            }
            ValidReason::TrustedDecision => {}
        },
        Decision::Spam(reason) => match reason {
            SpamReason::Poison(_) => {
                labels.push(MetricLabel(
                    CompactString::const_new("reason"),
                    CompactString::const_new("poison"),
                ));
                metrics.increment_classification_spam(labels);
                if let Some(asn) = c.asn {
                    metrics.increment_asn_hidden(asn);
                }
            }
            SpamReason::UnwantedASN(_) => {
                labels.push(MetricLabel(
                    CompactString::const_new("reason"),
                    CompactString::const_new("unwanted ASN"),
                ));
                metrics.increment_classification_spam(labels);
                if let Some(asn) = c.asn {
                    metrics.increment_asn_known(asn);
                }
            }
            SpamReason::UnwantedAgent(_) => {
                labels.push(MetricLabel(
                    CompactString::const_new("reason"),
                    CompactString::const_new("unwanted agent"),
                ));
                metrics.increment_classification_spam(labels);
                if let Some(asn) = c.asn {
                    metrics.increment_asn_known(asn);
                }
            }
            SpamReason::TrustedDecision => {}
        },
    }
}

impl Drop for Metrics {
    fn drop(&mut self) {
        if let Some(path) = self.persist_path.clone() {
            match write_persisted_metrics(&path, self) {
                Ok(_) => log::debug!("Persisted metrics to {path}"),
                Err(e) => log::error!("Failed to persist metrics to {path}: {e}"),
            }
        }
    }
}
