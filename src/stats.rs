use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use hickory_resolver::config::NameServerConfig;
use tokio::net::TcpStream;

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub dns: Option<DnsStats>,
    pub tcp: Option<TcpStats>,
    pub tls: Option<TlsStats>,
}

impl Stats {
    pub(crate) fn on_dns_start(&mut self) {
        if self.dns.is_some() {}
    }
}

#[derive(Debug, Clone, Default)]
pub struct DnsStats {
    pub hit_cache: bool,
    pub duration: Duration,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TcpStats {
    pub stats: Vec<TcpStat>,
    pub total_duration: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct TcpStat {
    pub dest: String,
    pub duration: Duration,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TlsStats {
    pub dest: String,
    pub duration: Duration,
    pub error: Option<String>,
}

pub trait Recorder {
    fn on_dns_start(&self, name_servers: &[NameServerConfig], host: &str);

    fn on_dns_done(
        &self,
        name_servers: &[NameServerConfig],
        host: &str,
        hit_cache: bool,
        result: crate::Result<&[SocketAddr]>,
    );

    fn on_tcp_start(&self, dest: &SocketAddr);

    fn on_tcp_done(&self, dest: &SocketAddr, stream: crate::Result<&TcpStream>);

    fn on_tls_start(&self, stream: &TcpStream);

    fn on_tls_done(&self);
}

pub struct StatsRecorder {
    inner: Arc<Mutex<StatsRecorderInner>>,
}

impl Recorder for StatsRecorder {
    fn on_dns_start(&self, name_servers: &[NameServerConfig], host: &str) {
        self.inner.lock().unwrap().start = Instant::now();
    }

    fn on_dns_done(
        &self,
        _name_servers: &[NameServerConfig],
        _host: &str,
        hit_cache: bool,
        result: crate::Result<&[SocketAddr]>,
    ) {
        let mut inner = self.inner.lock().unwrap();
        inner.dns_done = Instant::now();
        inner.dns_hit_cache = hit_cache;
        inner.dns_error = result.err().map(|e| e.to_string());
    }

    fn on_tcp_start(&self, dest: &SocketAddr) {}

    fn on_tcp_done(&self, dest: &SocketAddr, stream: crate::Result<&TcpStream>) {}

    fn on_tls_start(&self, stream: &TcpStream) {}

    fn on_tls_done(&self) {}
}

#[derive(Debug, Clone)]
struct StatsRecorderInner {
    start: Instant,
    dns_done: Instant,
    dns_hit_cache: bool,
    dns_error: Option<String>,
}
