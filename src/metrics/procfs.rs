use super::MetricLabel;
use super::format::{MetricValue, append_metric_label, append_metric_value};
use compact_str::CompactString;
use std::sync::LazyLock;

static PAGE_SIZE: LazyLock<i64> =
    LazyLock::new(|| unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.into());
static CLK_TICK: LazyLock<f64> =
    LazyLock::new(|| unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64);

pub fn append_procfs_metrics(output_buffer: &mut String) {
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
        if let Ok((fd_count, limits)) = p
            .fd_count()
            .and_then(|fd_count| p.limits().map(|limits| (fd_count, limits)))
        {
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
