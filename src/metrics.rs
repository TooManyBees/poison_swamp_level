use crate::classifier::{Classification, Decision, SpamReason, ValidReason};
use crate::config::Config;
use compact_str::{CompactString, ToCompactString};
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Default)]
pub struct Metrics {
    request_counter: HashMap<MetricKey, AtomicU64>,
    classification_spam_counter: HashMap<MetricKey, AtomicU64>,
    classification_valid_counter: HashMap<MetricKey, AtomicU64>,
    asn_known_counter: HashMap<MetricKey, AtomicU64>,
    asn_hidden_counter: HashMap<MetricKey, AtomicU64>,

    output_buffer: String,
}

impl Metrics {
    pub fn increment_request(&mut self, labels: Vec<MetricLabel>) {
        let key = MetricKey {
            name: CompactString::const_new("requests"),
            labels,
        };
        self.request_counter
            .entry(key)
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::Release);
    }

    pub fn increment_classification_spam(&mut self, labels: Vec<MetricLabel>) {
        let key = MetricKey {
            name: CompactString::const_new("classifications_spam"),
            labels,
        };
        self.classification_spam_counter
            .entry(key)
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::Release);
    }

    pub fn increment_classification_valid(&mut self, labels: Vec<MetricLabel>) {
        let key = MetricKey {
            name: CompactString::const_new("classifications_valid"),
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
            name: CompactString::const_new("asns_known"),
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
            name: CompactString::const_new("asns_hidden"),
            labels: vec![label],
        };
        self.asn_hidden_counter
            .entry(key)
            .or_insert(AtomicU64::new(0))
            .fetch_add(1, Ordering::Release);
    }

    pub fn to_prometheus(&mut self) -> String {
        let mut output_buffer = std::mem::take(&mut self.output_buffer);
        output_buffer.clear();
        for (key, inner) in &self.request_counter {
            append_metric_output(
                &mut output_buffer,
                &key.name,
                &key.labels,
                MetricValue::AtomicInt(inner),
                "counter",
                "Number of requests",
            );
        }
        for (key, inner) in &self.classification_spam_counter {
            append_metric_output(
                &mut output_buffer,
                &key.name,
                &key.labels,
                MetricValue::AtomicInt(inner),
                "counter",
                "Number of spam classifications",
            );
        }
        for (key, inner) in &self.classification_valid_counter {
            append_metric_output(
                &mut output_buffer,
                &key.name,
                &key.labels,
                MetricValue::AtomicInt(inner),
                "counter",
                "Number of valid classifications",
            );
        }
        for (key, inner) in &self.asn_known_counter {
            append_metric_output(
                &mut output_buffer,
                &key.name,
                &key.labels,
                MetricValue::AtomicInt(inner),
                "counter",
                "ASNs whence originate spam",
            );
        }
        for (key, inner) in &self.asn_hidden_counter {
            append_metric_output(
                &mut output_buffer,
                &key.name,
                &key.labels,
                MetricValue::AtomicInt(inner),
                "counter",
                "ASNs hiding spam",
            );
        }
        self.output_buffer = output_buffer;
        self.output_buffer.clone()
    }
}

#[allow(unused)]
enum MetricValue<'a> {
    Int(u64),
    AtomicInt(&'a AtomicU64),
    Float(f64),
}

fn append_metric_output(
    output_buffer: &mut String,
    key: &str,
    labels: &[MetricLabel],
    value: MetricValue,
    kind: &'static str,
    desc: &'static str,
) {
    output_buffer.push_str("# HELP ");
    output_buffer.push_str(key);
    output_buffer.push(' ');
    output_buffer.push_str(desc);
    output_buffer.push('\n');

    output_buffer.push_str("# TYPE ");
    output_buffer.push_str(key);
    output_buffer.push(' ');
    output_buffer.push_str(kind);
    output_buffer.push('\n');

    output_buffer.push_str(key);
    if !labels.is_empty() {
        output_buffer.push('{');
        for (n, MetricLabel(name, value)) in labels.iter().enumerate() {
            if n > 0 {
                output_buffer.push(',');
            }
            output_buffer.push_str(name);
            output_buffer.push_str("=\"");
            output_buffer.push_str(value);
            output_buffer.push('"');
        }
        output_buffer.push('}');
    }
    let _ = match value {
        MetricValue::Int(i) => writeln!(output_buffer, " {i}"),
        MetricValue::Float(f) => writeln!(output_buffer, " {f}"),
        MetricValue::AtomicInt(i) => {
            let value = i.load(Ordering::Acquire);
            writeln!(output_buffer, " {value}")
        }
    };
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MetricLabel(CompactString, CompactString);

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct MetricKey {
    name: CompactString,
    labels: Vec<MetricLabel>,
}

pub fn init(config: &Config) -> Arc<Mutex<Metrics>> {
    let metrics = Metrics::default();

    // TODO: load and set persisted metrics

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
