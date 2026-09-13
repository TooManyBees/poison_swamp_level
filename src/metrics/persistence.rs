use super::{MetricKey, MetricLabel, Metrics, NamedHashMap};
use compact_str::CompactString;
use std::sync::atomic::AtomicU64;
use std::{fmt, fs::File, io, io::ErrorKind, mem::take, path::Path};

type PersistedMetrics = Vec<(
    CompactString,
    Vec<(Vec<(CompactString, CompactString)>, u64)>,
)>;

pub enum PersistenceError {
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

pub fn load_persisted_metrics(path: &str) -> Result<Option<PersistedMetrics>, PersistenceError> {
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

pub fn apply_persisted_metrics(metrics: &mut Metrics, mut persisted: PersistedMetrics) {
    extract_persisted_metric(&mut metrics.request_counter, &mut persisted);
    extract_persisted_metric(&mut metrics.classification_spam_counter, &mut persisted);
    extract_persisted_metric(&mut metrics.classification_valid_counter, &mut persisted);
    extract_persisted_metric(&mut metrics.asn_known_counter, &mut persisted);
    extract_persisted_metric(&mut metrics.asn_hidden_counter, &mut persisted);
}

fn extract_persisted_metric(map: &mut NamedHashMap, persisted: &mut PersistedMetrics) {
    let metric_name = map.name.clone();
    if let Some((_, ms)) = persisted
        .extract_if(.., |(name, _)| *name == metric_name)
        .nth(0)
    {
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

pub fn write_persisted_metrics(path: &str, metrics: &mut Metrics) -> Result<(), PersistenceError> {
    let mut persisted: PersistedMetrics = Vec::with_capacity(5);

    metrics_into_persisted(&mut persisted, take(&mut metrics.request_counter));
    metrics_into_persisted(
        &mut persisted,
        take(&mut metrics.classification_spam_counter),
    );
    metrics_into_persisted(
        &mut persisted,
        take(&mut metrics.classification_valid_counter),
    );
    metrics_into_persisted(&mut persisted, take(&mut metrics.asn_known_counter));
    metrics_into_persisted(&mut persisted, take(&mut metrics.asn_hidden_counter));

    let parent_dir = Path::new(path).parent().unwrap_or(Path::new("."));
    let file = tempfile::NamedTempFile::new_in(parent_dir).map_err(PersistenceError::Io)?;
    serde_json::to_writer(&file, &persisted).map_err(PersistenceError::Json)?;
    file.into_temp_path()
        .persist(path)
        .map_err(|e| PersistenceError::Io(e.error))?;

    Ok(())
}

fn metrics_into_persisted(persisted: &mut PersistedMetrics, metric: NamedHashMap) {
    persisted.push((
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
    ));
}
