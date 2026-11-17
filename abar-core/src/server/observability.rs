use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(super) struct RuntimeMetrics {
    started_at: Instant,
    requests_total: AtomicU64,
    request_errors_total: AtomicU64,
    request_duration_micros_total: AtomicU64,
    worker_failures_total: AtomicU64,
    approvals_granted_total: AtomicU64,
    approvals_rejected_total: AtomicU64,
}

impl RuntimeMetrics {
    pub(super) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            requests_total: AtomicU64::new(0),
            request_errors_total: AtomicU64::new(0),
            request_duration_micros_total: AtomicU64::new(0),
            worker_failures_total: AtomicU64::new(0),
            approvals_granted_total: AtomicU64::new(0),
            approvals_rejected_total: AtomicU64::new(0),
        }
    }

    pub(super) fn record(&self, status: u16, duration: Duration) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.request_duration_micros_total.fetch_add(
            duration.as_micros().try_into().unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        if status >= 400 {
            self.request_errors_total.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub(super) fn record_worker_failure(&self) {
        self.worker_failures_total.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn record_approval(&self, approved: bool) {
        if approved {
            self.approvals_granted_total.fetch_add(1, Ordering::Relaxed);
        } else {
            self.approvals_rejected_total
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            requests_total: self.requests_total.load(Ordering::Relaxed),
            request_errors_total: self.request_errors_total.load(Ordering::Relaxed),
            request_duration_micros_total: self
                .request_duration_micros_total
                .load(Ordering::Relaxed),
            worker_failures_total: self.worker_failures_total.load(Ordering::Relaxed),
            approvals_granted_total: self.approvals_granted_total.load(Ordering::Relaxed),
            approvals_rejected_total: self.approvals_rejected_total.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct MetricsSnapshot {
    pub(super) requests_total: u64,
    pub(super) request_errors_total: u64,
    pub(super) request_duration_micros_total: u64,
    pub(super) worker_failures_total: u64,
    pub(super) approvals_granted_total: u64,
    pub(super) approvals_rejected_total: u64,
}

pub(super) async fn track_http(
    State(state): State<Arc<super::ServerState>>,
    request: Request,
    next: Next,
) -> Response {
    let started = Instant::now();
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    let status = response.status().as_u16();
    let elapsed = started.elapsed();
    state.metrics.record(status, elapsed);
    if status >= 400 {
        event(
            "warn",
            "http.request",
            json!({
                "method": method,
                "path": path,
                "status": status,
                "durationMs": elapsed.as_millis()
            }),
        );
    }
    response
}

pub(super) fn event(level: &str, event: &str, fields: Value) {
    let timestamp = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "unknown".to_owned());
    eprintln!(
        "{}",
        json!({
            "timestamp": timestamp,
            "level": level,
            "event": event,
            "fields": fields
        })
    );
}

pub(super) fn storage_writable(root: &std::path::Path) -> bool {
    let sequence = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = root.join(format!(".runtime-status-{}-{sequence}", std::process::id()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        std::io::Write::write_all(&mut file, b"ok")?;
        file.sync_all()
    })();
    let _ = std::fs::remove_file(path);
    result.is_ok()
}

#[cfg(test)]
mod tests {
    use super::RuntimeMetrics;
    use std::time::Duration;

    #[test]
    fn records_requests_and_errors() {
        let metrics = RuntimeMetrics::new();
        metrics.record(200, Duration::from_millis(4));
        metrics.record(500, Duration::from_millis(6));
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests_total, 2);
        assert_eq!(snapshot.request_errors_total, 1);
        assert_eq!(snapshot.request_duration_micros_total, 10_000);
        metrics.record_worker_failure();
        metrics.record_approval(true);
        metrics.record_approval(false);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.worker_failures_total, 1);
        assert_eq!(snapshot.approvals_granted_total, 1);
        assert_eq!(snapshot.approvals_rejected_total, 1);
    }
}
