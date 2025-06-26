use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use hickory_resolver::config::NameServerConfig;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

use crate::request::Request;

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub dns_stats: Stat,
    pub tcp_stats: Option<Vec<Stat>>,
    pub tls_stats: Option<Stat>,
    pub request_stats: Option<Stat>,
    pub total_duration: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct Stat {
    pub duration: Duration,
    pub extend: Option<String>,
    pub error: Option<String>,
}

pub trait Recorder {
    fn on_dns_start(&self, _request: &Request, _name_servers: &[NameServerConfig], _host: &str) {}

    fn on_dns_done(
        &self,
        _request: &Request,
        _name_servers: &[NameServerConfig],
        _host: &str,
        _result: Result<(&[SocketAddr], bool), String>,
    ) {
    }

    fn on_tcp_start(&self, _request: &Request, _dest: &SocketAddr) {}

    fn on_tcp_done(
        &self,
        _request: &Request,
        _dest: &SocketAddr,
        _stream: Result<&TcpStream, String>,
    ) {
    }

    fn on_tls_start(&self, _request: &Request, _stream: &TcpStream) {}

    fn on_tls_done(&self, _request: &Request, _stream: Result<&TlsStream<TcpStream>, String>) {}

    fn on_request_start(&self, _request: &Request) {}
}

#[derive(Clone)]
pub struct StatsRecorder {
    inner: Arc<Mutex<StatsRecorderInner>>,
}

impl Recorder for StatsRecorder {
    fn on_dns_start(&self, _request: &Request, _name_servers: &[NameServerConfig], _host: &str) {
        self.inner.lock().unwrap().dns_stat.start = Some(Instant::now());
    }

    fn on_dns_done(
        &self,
        _request: &Request,
        name_servers: &[NameServerConfig],
        _host: &str,
        result: Result<(&[SocketAddr], bool), String>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.dns_stat.done = Some(Instant::now());
        inner.dns_name_servers = name_servers
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(",");
        inner.dns_hit_cache = result.as_ref().is_ok_and(|v| v.1);
        inner.dns_stat.result = Some(result.map(|v| {
            v.0.iter()
                .map(|vv| vv.to_string())
                .collect::<Vec<_>>()
                .join(",")
        }));
    }

    fn on_tcp_start(&self, _request: &Request, dest: &SocketAddr) {
        let mut inner = self.inner.lock().unwrap();

        let tcp_stats = inner.tcp_stats.get_or_insert(HashMap::new());
        tcp_stats.insert(
            dest.to_string(),
            StatRecord {
                start: Some(Instant::now()),
                done: None,
                result: None,
            },
        );
    }

    fn on_tcp_done(
        &self,
        _request: &Request,
        dest: &SocketAddr,
        stream: Result<&TcpStream, String>,
    ) {
        let mut inner = self.inner.lock().unwrap();

        let tcp_stats = inner.tcp_stats.get_or_insert(HashMap::new());

        let dest = dest.to_string();
        if let Some(record) = tcp_stats.get_mut(&dest) {
            let now = Instant::now();
            record.done = Some(now);
            record.result = Some(stream.map(|_| dest));
        }
        // else {
        //     unreachable!()
        // }
    }

    fn on_tls_start(&self, _request: &Request, _stream: &TcpStream) {
        let mut inner = self.inner.lock().unwrap();

        _ = inner.tls_stat.insert(StatRecord {
            start: Some(Instant::now()),
            done: None,
            result: None,
        });
    }

    fn on_tls_done(&self, _request: &Request, stream: Result<&TlsStream<TcpStream>, String>) {
        let mut inner = self.inner.lock().unwrap();

        if let Some(record) = inner.tls_stat.as_mut() {
            let now = Instant::now();
            record.done = Some(now);
            record.result = Some(stream.map(|stream| {
                stream.get_ref().1.protocol_version().map_or_else(
                    || "unknown".to_string(),
                    |v| v.as_str().unwrap_or_default().to_string(),
                )
            }));
        }
    }

    fn on_request_start(&self, _request: &Request) {
        let mut inner = self.inner.lock().unwrap();

        _ = inner.request_stat.insert(StatRecord {
            start: Some(Instant::now()),
            done: None,
            result: None,
        });
    }
}

impl Default for StatsRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl StatsRecorder {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(StatsRecorderInner::default())),
        }
    }

    pub fn finish(&self) -> Stats {
        let inner = self.inner.lock().unwrap();

        let now = Instant::now();
        let mut stats = Stats::default();

        stats.dns_stats.duration = inner
            .dns_stat
            .done
            .map(|done| done.duration_since(inner.dns_stat.start()))
            .unwrap_or_default();
        if let Some(dns_result) = inner.dns_stat.result.as_ref() {
            match dns_result {
                Ok(v) => stats.dns_stats.extend = Some(v.clone()),
                Err(e) => stats.dns_stats.error = Some(e.clone()),
            }
        }

        if let Some(tcp_stats) = inner.tcp_stats.as_ref() {
            _ = stats.tcp_stats.insert(
                tcp_stats
                    .iter()
                    .map(|(key, value)| {
                        let duration = value
                            .done
                            .map(|done| done.duration_since(value.start()))
                            .unwrap_or_default();
                        let extend = Some(key.clone());
                        let error = value
                            .result
                            .as_ref()
                            .and_then(|v| v.as_ref().err().cloned());

                        Stat {
                            duration,
                            extend,
                            error,
                        }
                    })
                    .collect(),
            );
        }

        if let Some(tls_stats) = inner.tls_stat.as_ref() {
            _ = stats.tls_stats.insert({
                let duration = tls_stats
                    .done
                    .map(|done| done.duration_since(tls_stats.start()))
                    .unwrap_or_default();
                let extend = tls_stats
                    .result
                    .as_ref()
                    .and_then(|v| v.as_ref().ok().cloned());
                let error = tls_stats
                    .result
                    .as_ref()
                    .and_then(|v| v.as_ref().err().cloned());

                Stat {
                    duration,
                    extend,
                    error,
                }
            });
        }

        if let Some(request_stats) = inner.request_stat.as_ref() {
            _ = stats.request_stats.insert({
                let duration = now.duration_since(request_stats.start());
                let extend = request_stats
                    .result
                    .as_ref()
                    .and_then(|v| v.as_ref().ok().cloned());
                let error = request_stats
                    .result
                    .as_ref()
                    .and_then(|v| v.as_ref().err().cloned());

                Stat {
                    duration,
                    extend,
                    error,
                }
            });
        }
        stats.total_duration = now.duration_since(inner.dns_stat.start());
        stats
    }
}
#[derive(Debug, Clone, Default)]
struct StatsRecorderInner {
    dns_stat: StatRecord,
    dns_hit_cache: bool,
    dns_name_servers: String,

    tcp_stats: Option<HashMap<String, StatRecord>>,
    tls_stat: Option<StatRecord>,
    request_stat: Option<StatRecord>,
}

#[derive(Debug, Clone, Default)]
struct StatRecord {
    start: Option<Instant>,
    done: Option<Instant>,
    result: Option<Result<String, String>>,
}

impl StatRecord {
    fn start(&self) -> Instant {
        // ok or far future
        self.start
            .unwrap_or(Instant::now() + Duration::from_secs(86400 * 365 * 30))
    }
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "total_duration:   {:>4}ms",
            self.total_duration.as_millis()
        )?;
        writeln!(
            f,
            "dns_duration:     {:>4}ms >>> resolve: {}",
            self.dns_stats.duration.as_millis(),
            self.dns_stats.extend.clone().unwrap_or_default(),
        )?;

        if let Some(tcp_stats) = self.tcp_stats.as_ref() {
            for stat in tcp_stats {
                let duration = stat.duration.as_millis();
                let extend = stat.extend.clone().unwrap_or_default();
                write!(
                    f,
                    "tcp_duration:     {:>4}ms >>> connect: {} ",
                    duration, extend
                )?;
                if let Some(error) = &stat.error {
                    write!(f, "; failed: {}", error)?;
                }
                writeln!(f)?;
            }
        }
        if let Some(tls_stats) = self.tls_stats.as_ref() {
            let duration = tls_stats.duration.as_millis();
            let extend = tls_stats.extend.clone().unwrap_or_default();
            write!(
                f,
                "tls_duration:     {:>4}ms >>> version: {} ",
                duration, extend
            )?;
            if let Some(error) = &tls_stats.error {
                write!(f, "; failed: {}", error)?;
            }
            writeln!(f)?;
        }

        if let Some(stats) = self.request_stats.as_ref() {
            let duration = stats.duration.as_millis();
            write!(f, "request_duration: {:>4}ms", duration)?;
            if let Some(error) = &stats.error {
                write!(f, " failed: {}", error)?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}
