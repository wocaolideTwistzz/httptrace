use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Once},
    time::Duration,
};

use arc_swap::ArcSwap;
use hickory_resolver::{
    Resolver, TokioResolver,
    config::{LookupIpStrategy, NameServerConfig, ResolverConfig},
    name_server::{GenericConnector, TokioConnectionProvider},
    proto::runtime::TokioRuntimeProvider,
};

use crate::request::Request;

#[derive(Clone, Debug)]
pub struct Client {
    inner: Arc<ArcSwap<ClientRef>>,
}

impl Client {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub async fn do_request(&self, request: Request) -> crate::Result<()> {
        todo!()
    }

    async fn dns_resolve(&self, request: &mut Request) -> crate::Result<()> {
        let ret = self.inner.load().resolver.lookup_ip("host").await.unwrap();
        todo!()
    }
}

#[derive(Clone, Debug)]
pub struct ClientRef {
    local_addr: Option<SocketAddr>,
    resolver: Resolver<GenericConnector<TokioRuntimeProvider>>,
    dns_overrides: HashMap<String, Vec<SocketAddr>>,

    dns_timeout: Option<Duration>,
    tcp_timeout: Option<Duration>,
    tls_timeout: Option<Duration>,
}

#[derive(Debug, Clone, Default)]
pub struct ClientBuilder {
    local_addr: Option<SocketAddr>,
    lookup_ip_strategy: Option<LookupIpStrategy>,
    name_servers: Option<Vec<NameServerConfig>>,
    headers: Option<http::HeaderMap>,
    skip_verify: bool,
    dns_overrides: HashMap<String, Vec<SocketAddr>>,

    dns_timeout: Option<Duration>,
    tcp_timeout: Option<Duration>,
    tls_timeout: Option<Duration>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        ClientBuilder::default()
    }

    pub fn build(self) -> crate::error::Result<Client> {
        let resolver = {
            let provider = TokioConnectionProvider::default();
            if self.name_servers.as_ref().is_some_and(|v| !v.is_empty()) {
                let mut config = ResolverConfig::new();
                for ns in self.name_servers.unwrap() {
                    config.add_name_server(ns);
                }
                TokioResolver::builder_with_config(config, provider).build()
            } else {
                TokioResolver::builder(provider)?.build()
            }
        };

        Ok(Client {
            inner: Arc::new(ArcSwap::from_pointee(ClientRef {
                resolver,
                local_addr: self.local_addr,
                dns_overrides: self.dns_overrides,
                dns_timeout: self.dns_timeout,
                tcp_timeout: self.tcp_timeout,
                tls_timeout: self.tls_timeout,
            })),
        })
    }

    pub fn local_addr(mut self, addr: SocketAddr) -> Self {
        self.local_addr = Some(addr);
        self
    }

    pub fn resolve_to_addrs(mut self, domain: &str, addrs: &[SocketAddr]) -> Self {
        self.dns_overrides
            .insert(domain.to_string(), addrs.to_vec());
        self
    }

    pub fn lookup_ip_strategy(mut self, strategy: LookupIpStrategy) -> Self {
        self.lookup_ip_strategy = Some(strategy);
        self
    }

    pub fn name_servers<I>(mut self, addr: I) -> Self
    where
        I: IntoIterator<Item = NameServerConfig>,
    {
        self.name_servers = Some(addr.into_iter().collect());
        self
    }

    pub fn headers(mut self, headers: http::HeaderMap) -> Self {
        self.headers = Some(headers);
        self
    }

    pub fn dns_timeout(mut self, timeout: Duration) -> Self {
        self.dns_timeout = Some(timeout);
        self
    }

    pub fn tcp_timeout(mut self, timeout: Duration) -> Self {
        self.tcp_timeout = Some(timeout);
        self
    }

    pub fn tls_timeout(mut self, timeout: Duration) -> Self {
        self.tls_timeout = Some(timeout);
        self
    }

    pub fn skip_tls_verify(mut self) -> Self {
        self.skip_verify = true;
        self
    }
}

static INIT_TLS: Once = Once::new();

fn ensure_crypto_provider() {
    INIT_TLS.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    });
}
