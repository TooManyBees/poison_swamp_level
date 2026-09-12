use super::{MetricLabel, NamedHashMap};
use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};

pub enum MetricValue<'a> {
    #[allow(unused)]
    Int(i64),
    AtomicInt(&'a AtomicU64),
    #[allow(unused)]
    Float(f64),
}

pub fn append_metric(output_buffer: &mut String, metric: &NamedHashMap) {
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

pub fn append_metric_label(
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

pub fn append_metric_value(
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
