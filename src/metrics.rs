use crate::classifier::{Classification, Decision, SpamReason, ValidReason};
use crate::config::Config;
use compact_str::{CompactString, ToCompactString};
use std::collections::HashMap;
use std::fmt::Write;
#[cfg(target_os = "linux")]
use std::sync::LazyLock;
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
        if !self.request_counter.is_empty() {
            append_metric_label(
                &mut output_buffer,
                "requests",
                "counter",
                "Number of requests",
            );
        }
        for (key, inner) in &self.request_counter {
            append_metric_value(
                &mut output_buffer,
                "requests",
                &key.labels,
                MetricValue::AtomicInt(inner),
            );
        }

        if !self.classification_spam_counter.is_empty() {
            append_metric_label(
                &mut output_buffer,
                "classifications_spam",
                "counter",
                "Number of spam classifications",
            );
        }
        for (key, inner) in &self.classification_spam_counter {
            append_metric_value(
                &mut output_buffer,
                "classifications_spam",
                &key.labels,
                MetricValue::AtomicInt(inner),
            );
        }

        if !self.classification_valid_counter.is_empty() {
            append_metric_label(
                &mut output_buffer,
                "classifications_valid",
                "counter",
                "Number of valid classifications",
            );
        }
        for (key, inner) in &self.classification_valid_counter {
            append_metric_value(
                &mut output_buffer,
                "classifications_valid",
                &key.labels,
                MetricValue::AtomicInt(inner),
            );
        }

        if !self.asn_known_counter.is_empty() {
            append_metric_label(
                &mut output_buffer,
                "asns_known",
                "counter",
                "ASNs whence originate spam",
            );
        }
        for (key, inner) in &self.asn_known_counter {
            append_metric_value(
                &mut output_buffer,
                "asns_known",
                &key.labels,
                MetricValue::AtomicInt(inner),
            );
        }

        if !self.asn_hidden_counter.is_empty() {
            append_metric_label(
                &mut output_buffer,
                "asns_hidden",
                "counter",
                "ASNs hiding spam",
            );
        }
        for (key, inner) in &self.asn_hidden_counter {
            append_metric_value(
                &mut output_buffer,
                "asns_hidden",
                &key.labels,
                MetricValue::AtomicInt(inner),
            );
        }

        #[cfg(target_os = "linux")]
        append_procfs_metrics(&mut output_buffer);

        self.output_buffer = output_buffer;
        self.output_buffer.clone()
    }
}

#[allow(unused)]
enum MetricValue<'a> {
    Int(i64),
    AtomicInt(&'a AtomicU64),
    Float(f64),
}

fn append_metric_label(
    output_buffer: &mut String,
    key: &str,
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
}

fn append_metric_value(
    output_buffer: &mut String,
    key: &str,
    labels: &[MetricLabel],
    value: MetricValue,
) {
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

#[cfg(target_os = "linux")]
static PAGE_SIZE: LazyLock<i64> =
    LazyLock::new(|| unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.into());
#[cfg(target_os = "linux")]
static CLK_TICK: LazyLock<f64> =
    LazyLock::new(|| unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64);

#[cfg(target_os = "linux")]
fn append_procfs_metrics(output_buffer: &mut String) {
    if let Ok(p) = procfs::process::Process::myself() {
        if let Ok(stat) = p.stat() {
            append_metric_label(
                output_buffer,
                "threads",
                "gauge",
                "Number of OS threads in the process",
            );
            append_metric_value(
                output_buffer,
                "threads",
                &[],
                MetricValue::Int(stat.num_threads as i64),
            );
            append_metric_label(
                output_buffer,
                "vss_bytes",
                "gauge",
                "Virtual memory size in bytes",
            );
            append_metric_value(
                output_buffer,
                "vss_bytes",
                &[],
                MetricValue::Int(stat.vsize as i64),
            );
            append_metric_label(
                output_buffer,
                "rss_bytes",
                "gauge",
                "Resident memory size in bytes",
            );
            append_metric_value(
                output_buffer,
                "rss_bytes",
                &[],
                MetricValue::Int(stat.rss as i64 * *PAGE_SIZE),
            );
            append_metric_label(
                output_buffer,
                "cpu_time_seconds",
                "gauge",
                "Total user and system CPU time in seconds",
            );
            append_metric_value(
                output_buffer,
                "cpu_time_seconds",
                &[],
                MetricValue::Float((stat.utime + stat.stime) as f64 / *CLK_TICK),
            );
        }
        if let Ok((fd_count, limits)) = p.fd_count().zip(p.limits()) {
            append_metric_label(
                output_buffer,
                "fds",
                "gauge",
                "Number of open file descriptors",
            );
            append_metric_value(
                output_buffer,
                "fds",
                &[MetricLabel(
                    CompactString::const_new("fds"),
                    CompactString::const_new("open"),
                )],
                MetricValue::Int(fd_count as i64),
            );
            if let procfs::process::LimitValue::Value(max) = limits.max_open_files.soft_limit {
                append_metric_value(
                    output_buffer,
                    "fds",
                    &[MetricLabel(
                        CompactString::const_new("fds"),
                        CompactString::const_new("max"),
                    )],
                    MetricValue::Int(max as i64),
                );
            }
        }
    }
}
