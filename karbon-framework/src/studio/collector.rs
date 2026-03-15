use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, RwLock};
use serde::Serialize;

const MAX_REQUESTS: usize = 500;
const MAX_EVENTS: usize = 200;
const MAX_JOBS: usize = 200;
const MAX_MAILS: usize = 100;

/// Sensitive header names that should be masked in the studio
const SENSITIVE_HEADERS: &[&str] = &[
    "authorization",
    "cookie",
    "set-cookie",
    "x-api-key",
    "x-auth-token",
];

/// Sensitive patterns in header values
const SENSITIVE_PATTERNS: &[&str] = &[
    "secret",
    "password",
    "token",
    "key",
];

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn mask_header_value(name: &str, value: &str) -> String {
    let lower = name.to_lowercase();
    if SENSITIVE_HEADERS.contains(&lower.as_str())
        || SENSITIVE_PATTERNS.iter().any(|p| lower.contains(p))
        || value.to_lowercase().starts_with("bearer ")
        || value.to_lowercase().starts_with("basic ")
    {
        "••••••••".to_string()
    } else {
        value.to_string()
    }
}

/// A recorded HTTP request/response pair
#[derive(Debug, Clone, Serialize)]
pub struct RequestRecord {
    pub id: u64,
    pub timestamp: u64,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub duration_ms: u64,
    pub request_headers: Vec<(String, String)>,
    pub response_headers: Vec<(String, String)>,
    pub request_id: Option<String>,
    pub remote_addr: Option<String>,
}

/// A recorded event emission
#[derive(Debug, Clone, Serialize)]
pub struct EventRecord {
    pub id: u64,
    pub timestamp: u64,
    pub event_type: String,
    pub handler_count: usize,
}

/// A recorded job execution
#[derive(Debug, Clone, Serialize)]
pub struct JobRecord {
    pub id: u64,
    pub timestamp: u64,
    pub name: String,
    pub status: JobStatus,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
    pub attempt: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

/// A recorded mail send
#[derive(Debug, Clone, Serialize)]
pub struct MailRecord {
    pub id: u64,
    pub timestamp: u64,
    pub to: Vec<String>,
    pub subject: String,
    pub status: MailStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MailStatus {
    Sent,
    Failed,
}

/// A message broadcast to WebSocket clients
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum StudioMessage {
    #[serde(rename = "request")]
    Request(RequestRecord),
    #[serde(rename = "event")]
    Event(EventRecord),
    #[serde(rename = "job")]
    Job(JobRecord),
    #[serde(rename = "mail")]
    Mail(MailRecord),
    #[serde(rename = "stats")]
    Stats(StudioStats),
}

#[derive(Debug, Clone, Serialize)]
pub struct StudioStats {
    pub total_requests: u64,
    pub total_events: u64,
    pub total_jobs: u64,
    pub total_mails: u64,
    pub avg_response_ms: f64,
    pub error_rate: f64,
    pub uptime_secs: u64,
}

struct StudioData {
    requests: VecDeque<RequestRecord>,
    events: VecDeque<EventRecord>,
    jobs: VecDeque<JobRecord>,
    mails: VecDeque<MailRecord>,
    next_id: u64,
    total_requests: u64,
    total_events: u64,
    total_jobs: u64,
    total_mails: u64,
    total_duration_ms: u64,
    total_errors: u64,
    started_at: u64,
}

/// Central collector for all studio data.
/// Thread-safe, bounded, in-memory only.
#[derive(Clone)]
pub struct StudioCollector {
    data: Arc<RwLock<StudioData>>,
    tx: broadcast::Sender<String>,
}

impl StudioCollector {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            data: Arc::new(RwLock::new(StudioData {
                requests: VecDeque::with_capacity(MAX_REQUESTS),
                events: VecDeque::with_capacity(MAX_EVENTS),
                jobs: VecDeque::with_capacity(MAX_JOBS),
                mails: VecDeque::with_capacity(MAX_MAILS),
                next_id: 1,
                total_requests: 0,
                total_events: 0,
                total_jobs: 0,
                total_mails: 0,
                total_duration_ms: 0,
                total_errors: 0,
                started_at: now_ms(),
            })),
            tx,
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    fn broadcast(&self, msg: &StudioMessage) {
        if let Ok(json) = serde_json::to_string(msg) {
            let _ = self.tx.send(json);
        }
    }

    pub async fn record_request(
        &self,
        method: String,
        path: String,
        status: u16,
        duration: Duration,
        request_headers: Vec<(String, String)>,
        response_headers: Vec<(String, String)>,
        request_id: Option<String>,
        remote_addr: Option<String>,
    ) {
        let masked_req_headers: Vec<(String, String)> = request_headers
            .into_iter()
            .map(|(k, v)| {
                let masked = mask_header_value(&k, &v);
                (k, masked)
            })
            .collect();

        let masked_res_headers: Vec<(String, String)> = response_headers
            .into_iter()
            .map(|(k, v)| {
                let masked = mask_header_value(&k, &v);
                (k, masked)
            })
            .collect();

        let mut data = self.data.write().await;
        let id = data.next_id;
        data.next_id += 1;
        data.total_requests += 1;
        data.total_duration_ms += duration.as_millis() as u64;
        if status >= 400 {
            data.total_errors += 1;
        }

        let record = RequestRecord {
            id,
            timestamp: now_ms(),
            method,
            path,
            status,
            duration_ms: duration.as_millis() as u64,
            request_headers: masked_req_headers,
            response_headers: masked_res_headers,
            request_id,
            remote_addr,
        };

        if data.requests.len() >= MAX_REQUESTS {
            data.requests.pop_front();
        }
        data.requests.push_back(record.clone());
        drop(data);

        self.broadcast(&StudioMessage::Request(record));
    }

    pub async fn record_event(&self, event_type: String, handler_count: usize) {
        let mut data = self.data.write().await;
        let id = data.next_id;
        data.next_id += 1;
        data.total_events += 1;

        let record = EventRecord {
            id,
            timestamp: now_ms(),
            event_type,
            handler_count,
        };

        if data.events.len() >= MAX_EVENTS {
            data.events.pop_front();
        }
        data.events.push_back(record.clone());
        drop(data);

        self.broadcast(&StudioMessage::Event(record));
    }

    pub async fn record_job(
        &self,
        name: String,
        status: JobStatus,
        duration_ms: Option<u64>,
        error: Option<String>,
        attempt: u32,
    ) {
        let mut data = self.data.write().await;
        let id = data.next_id;
        data.next_id += 1;
        data.total_jobs += 1;

        let record = JobRecord {
            id,
            timestamp: now_ms(),
            name,
            status,
            duration_ms,
            error,
            attempt,
        };

        if data.jobs.len() >= MAX_JOBS {
            data.jobs.pop_front();
        }
        data.jobs.push_back(record.clone());
        drop(data);

        self.broadcast(&StudioMessage::Job(record));
    }

    pub async fn record_mail(
        &self,
        to: Vec<String>,
        subject: String,
        status: MailStatus,
        error: Option<String>,
    ) {
        let mut data = self.data.write().await;
        let id = data.next_id;
        data.next_id += 1;
        data.total_mails += 1;

        let record = MailRecord {
            id,
            timestamp: now_ms(),
            to,
            subject,
            status,
            error,
        };

        if data.mails.len() >= MAX_MAILS {
            data.mails.pop_front();
        }
        data.mails.push_back(record.clone());
        drop(data);

        self.broadcast(&StudioMessage::Mail(record));
    }

    pub async fn get_stats(&self) -> StudioStats {
        let data = self.data.read().await;
        let avg = if data.total_requests > 0 {
            data.total_duration_ms as f64 / data.total_requests as f64
        } else {
            0.0
        };
        let error_rate = if data.total_requests > 0 {
            (data.total_errors as f64 / data.total_requests as f64) * 100.0
        } else {
            0.0
        };
        let uptime = (now_ms() - data.started_at) / 1000;

        StudioStats {
            total_requests: data.total_requests,
            total_events: data.total_events,
            total_jobs: data.total_jobs,
            total_mails: data.total_mails,
            avg_response_ms: (avg * 100.0).round() / 100.0,
            error_rate: (error_rate * 100.0).round() / 100.0,
            uptime_secs: uptime,
        }
    }

    pub async fn get_requests(&self) -> Vec<RequestRecord> {
        self.data.read().await.requests.iter().rev().cloned().collect()
    }

    pub async fn get_events(&self) -> Vec<EventRecord> {
        self.data.read().await.events.iter().rev().cloned().collect()
    }

    pub async fn get_jobs(&self) -> Vec<JobRecord> {
        self.data.read().await.jobs.iter().rev().cloned().collect()
    }

    pub async fn get_mails(&self) -> Vec<MailRecord> {
        self.data.read().await.mails.iter().rev().cloned().collect()
    }

    pub async fn clear(&self) {
        let mut data = self.data.write().await;
        data.requests.clear();
        data.events.clear();
        data.jobs.clear();
        data.mails.clear();
    }
}

impl Default for StudioCollector {
    fn default() -> Self {
        Self::new()
    }
}
