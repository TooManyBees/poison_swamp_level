use crate::classifier::{Classification, Decision, SpamReason, ValidReason};
use crate::config::Config;
use compact_str::{CompactString, ToCompactString};
use std::collections::HashMap;
use std::fmt::{self, Write};
use std::fs::File;
use std::io::{self, ErrorKind};
#[cfg(target_os = "linux")]
use std::sync::LazyLock;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Default)]
struct NamedHashMap {
    inner: HashMap<MetricKey, AtomicU64>,
    name: CompactString,
    desc: &'static str,
}

impl std::ops::Deref for NamedHashMap {
    type Target = HashMap<MetricKey, AtomicU64>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for NamedHashMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl NamedHashMap {
    fn new(name: &'static str, desc: &'static str) -> Self {
        NamedHashMap {
            inner: HashMap::new(),
            name: CompactString::const_new(name),
            desc,
        }
    }
}

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
        let mut output_buffer = std::mem::take(&mut self.output_buffer);
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

#[allow(unused)]
enum MetricValue<'a> {
    Int(i64),
    AtomicInt(&'a AtomicU64),
    Float(f64),
}

fn append_metric(output_buffer: &mut String, metric: &NamedHashMap) {
    if metric.is_empty() {
        return;
    }

    append_metric_label(output_buffer, &metric.name, "counter", metric.desc);
    for (key, inner) in metric.iter() {
        append_metric_value(
            output_buffer,
            &metric.name,
            &key.labels,
            MetricValue::AtomicInt(inner),
        )
    }
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

type PersistedMetrics = HashMap<CompactString, Vec<(Vec<(CompactString, CompactString)>, u64)>>;

enum PersistenceError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PersistenceError::Io(e) => e.fmt(f),
            PersistenceError::Json(e) => e.fmt(f),
        }
    }
}

fn load_persisted_metrics(path: &str) -> Result<Option<PersistedMetrics>, PersistenceError> {
    let f = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            if e.kind() == ErrorKind::NotFound {
                return Ok(None);
            } else {
                return Err(PersistenceError::Io(e));
            }
        }
    };
    let persisted: PersistedMetrics = serde_json::from_reader(f).map_err(PersistenceError::Json)?;
    Ok(Some(persisted))
}

fn apply_persisted_metrics(metrics: &mut Metrics, mut persisted: PersistedMetrics) {
    extract_persisted_metric(&mut metrics.request_counter, &mut persisted);
    extract_persisted_metric(&mut metrics.classification_spam_counter, &mut persisted);
    extract_persisted_metric(&mut metrics.classification_valid_counter, &mut persisted);
    extract_persisted_metric(&mut metrics.asn_known_counter, &mut persisted);
    extract_persisted_metric(&mut metrics.asn_hidden_counter, &mut persisted);
}

fn extract_persisted_metric(map: &mut NamedHashMap, persisted: &mut PersistedMetrics) {
    if let Some(ms) = persisted.remove(&map.name) {
        for (labels, value) in ms {
            let labels = labels
                .into_iter()
                .map(|(name, value)| MetricLabel(name, value))
                .collect();
            let key = MetricKey {
                name: map.name.clone(),
                labels,
            };
            map.insert(key, AtomicU64::from(value));
        }
    }
}

fn write_persisted_metrics(path: &str, metrics: &mut Metrics) -> Result<(), PersistenceError> {
    let mut persisted: PersistedMetrics = HashMap::new();

    metrics_into_persisted(&mut persisted, std::mem::take(&mut metrics.request_counter));
    metrics_into_persisted(
        &mut persisted,
        std::mem::take(&mut metrics.classification_spam_counter),
    );
    metrics_into_persisted(
        &mut persisted,
        std::mem::take(&mut metrics.classification_valid_counter),
    );
    metrics_into_persisted(
        &mut persisted,
        std::mem::take(&mut metrics.asn_known_counter),
    );
    metrics_into_persisted(
        &mut persisted,
        std::mem::take(&mut metrics.asn_hidden_counter),
    );

    let f = File::create(path).map_err(PersistenceError::Io)?;
    serde_json::to_writer_pretty(f, &persisted).map_err(PersistenceError::Json)?;

    Ok(())
}

fn metrics_into_persisted(persisted: &mut PersistedMetrics, metric: NamedHashMap) {
    persisted.insert(
        metric.name.clone(),
        metric
            .inner
            .into_iter()
            .map(|(key, value)| {
                let labels = key
                    .labels
                    .into_iter()
                    .map(|MetricLabel(name, val)| (name, val))
                    .collect();
                (labels, value.into_inner())
            })
            .collect(),
    );
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
