use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Once},
    time::Duration,
};

use hickory_resolver::{
    Resolver, TokioResolver,
    config::{LookupIpStrategy, NameServerConfig, ResolverConfig},
    name_server::{GenericConnector, TokioConnectionProvider},
    proto::runtime::TokioRuntimeProvider,
};
use http::{HeaderValue, Method};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::{ClientConfig, RootCertStore};
use tokio::{
    net::{TcpSocket, TcpStream},
    time::Instant,
};
use tokio_rustls::{TlsConnector, client::TlsStream};

use crate::{
    into_uri::IntoUri,
    request::{Request, RequestBuilder},
    response::Response,
    skip_verify::SkipVerifier,
};

#[derive(Clone, Debug)]
pub struct Client {
    inner: Arc<ClientRef>,
}

impl Client {
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub fn get<U: IntoUri>(&self, u: U) -> RequestBuilder {
        self.request(Method::GET, u)
    }

    pub fn post<U: IntoUri>(&self, u: U) -> RequestBuilder {
        self.request(Method::POST, u)
    }

    pub fn head<U: IntoUri>(&self, u: U) -> RequestBuilder {
        self.request(Method::HEAD, u)
    }

    pub fn request<U: IntoUri>(&self, method: Method, u: U) -> RequestBuilder {
        let req = u.into_uri().map(|uri| Request::new(method, uri));
        RequestBuilder::new(self.clone(), req)
    }

    pub async fn execute(&self, request: Request) -> crate::Result<Response> {
        self.inner.execute(request).await
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ClientRef {
    local_addr: Option<IpAddr>,
    resolver: Resolver<GenericConnector<TokioRuntimeProvider>>,
    dns_overrides: HashMap<String, Vec<IpAddr>>,
    skip_tls_verify: bool,
    alpn_protocols: Option<Vec<Alpn>>,
    disable_auto_set_header: bool,
    prefer_ipv6: bool,

    dns_timeout: Duration,
    tcp_timeout: Duration,
    tls_timeout: Duration,
}

#[derive(Debug, Clone, Default)]
pub struct ClientBuilder {
    local_addr: Option<IpAddr>,
    lookup_ip_strategy: Option<LookupIpStrategy>,
    name_servers: Option<Vec<NameServerConfig>>,
    headers: Option<http::HeaderMap>,
    skip_tls_verify: bool,
    disable_auto_set_header: bool,
    alpn_protocols: Option<Vec<Alpn>>,
    dns_overrides: HashMap<String, Vec<IpAddr>>,

    dns_timeout: Option<Duration>,
    tcp_timeout: Option<Duration>,
    tls_timeout: Option<Duration>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        ClientBuilder::default()
    }

    pub fn build(self) -> crate::error::Result<Client> {
        let mut resolver_builder = {
            let provider = TokioConnectionProvider::default();
            if self.name_servers.as_ref().is_some_and(|v| !v.is_empty()) {
                let mut config = ResolverConfig::new();
                for ns in self.name_servers.unwrap() {
                    config.add_name_server(ns);
                }
                TokioResolver::builder_with_config(config, provider)
            } else {
                TokioResolver::builder(provider)?
            }
        };

        resolver_builder.options_mut().ip_strategy = self.lookup_ip_strategy.unwrap_or_default();

        Ok(Client {
            inner: Arc::new(ClientRef {
                resolver: resolver_builder.build(),
                local_addr: self.local_addr,
                skip_tls_verify: self.skip_tls_verify,
                alpn_protocols: self.alpn_protocols,
                disable_auto_set_header: self.disable_auto_set_header,
                dns_overrides: self.dns_overrides,
                dns_timeout: self.dns_timeout.unwrap_or(FAR_INTERVAL), // or far future
                tcp_timeout: self.tcp_timeout.unwrap_or(FAR_INTERVAL), // or far future
                tls_timeout: self.tls_timeout.unwrap_or(FAR_INTERVAL), // or far future
                prefer_ipv6: self.lookup_ip_strategy.is_some_and(|v| {
                    v == LookupIpStrategy::Ipv6Only || v == LookupIpStrategy::Ipv6thenIpv4
                }),
            }),
        })
    }

    pub fn local_addr(mut self, addr: IpAddr) -> Self {
        self.local_addr = Some(addr);
        self
    }

    pub fn resolve_to_addrs(mut self, domain: &str, addrs: &[IpAddr]) -> Self {
        self.dns_overrides
            .insert(domain.to_string(), addrs.to_vec());
        self
    }

    pub fn lookup_ip_strategy(mut self, strategy: LookupIpStrategy) -> Self {
        self.lookup_ip_strategy = Some(strategy);
        self
    }

    pub fn alpn_protocols(mut self, alpn: Vec<Alpn>) -> Self {
        self.alpn_protocols = Some(alpn);
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
        self.skip_tls_verify = true;
        self
    }

    pub fn disable_auto_set_header(mut self) -> Self {
        self.disable_auto_set_header = true;
        self
    }
}

impl ClientRef {
    pub(crate) async fn execute(&self, mut request: Request) -> crate::Result<Response> {
        let timeout = *request.timeout().unwrap_or(&FAR_INTERVAL);

        tokio::time::timeout(timeout, async {
            let (addrs, _) = self.dns_resolve(&request).await?;

            let is_https = request.uri().scheme() == Some(&http::uri::Scheme::HTTPS);

            let stream = self.tcp_connect(&request, addrs).await?;

            if !self.disable_auto_set_header {
                let host = request.uri().host().ok_or(crate::Error::EmptyResolveResult)?.to_string();
                if request.headers().get(http::header::HOST).is_none() {
                    request
                        .headers_mut()
                        .insert(http::header::HOST, host.parse()?);
                }
                if request.headers().get(http::header::USER_AGENT).is_none() {
                    request.headers_mut().insert(http::header::USER_AGENT, HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36"));
                }
            }

            if is_https {
                let tls_stream = self.tls_handshake(stream, &request).await?;

                self.tls_send_request(tls_stream, request).await
            } else {
                self.tcp_send_h1_request(stream, request).await
            }
        })
        .await?
    }

    pub(crate) async fn dns_resolve(
        &self,
        request: &Request,
    ) -> crate::Result<(Vec<SocketAddr>, bool)> {
        let host = request.uri().host().ok_or(crate::Error::HostRequired)?;
        if let Some(recorder) = request.recorder() {
            recorder.on_dns_start(self.resolver.config().name_servers(), host);
        }

        let ret = self._dns_resolve(request).await;

        if let Some(recorder) = request.recorder() {
            recorder.on_dns_done(
                self.resolver.config().name_servers(),
                host,
                ret.as_ref()
                    .map(|(ips, hit_cache)| (ips.as_slice(), *hit_cache))
                    .map_err(|e| e.to_string()),
            );
        }
        ret
    }

    pub(crate) async fn tcp_connect(
        &self,
        request: &Request,
        addrs: Vec<SocketAddr>,
    ) -> crate::Result<TcpStream> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(SocketAddr, crate::Result<TcpStream>)>(1);
        let (cancel, _) = tokio::sync::broadcast::channel::<()>(1);

        let mut addrs = addrs.into_iter();

        let mut result: crate::Result<TcpStream> = Err(crate::Error::Unknown);
        let mut timer = Instant::now();
        let mut tx_opt = Some(tx);
        let deadline = timer + self.tcp_timeout;

        'outer: loop {
            tokio::select! {
                _ = tokio::time::sleep_until(deadline) => {
                    result = Err(crate::Error::TcpDeadlineExceeded);
                    break 'outer;
                }
                _ = tokio::time::sleep_until(timer) => {
                    match addrs.next() {
                        Some(addr) => {
                            if let Some(recorder) = request.recorder() {
                                recorder.on_tcp_start(&addr);
                            }
                            if let Some(tx) = tx_opt.clone() {
                                let local_addr = self.local_addr;
                                let prefer_ipv6 = self.prefer_ipv6;
                                let cancel_rx = cancel.subscribe();
                                tokio::spawn(async move {
                                    let ret = Self::_tcp_connect(local_addr, addr, cancel_rx, prefer_ipv6).await;
                                    _ = tx.send((addr, ret)).await;
                                });
                            }
                            timer += FALLBACK_INTERVAL;
                        }
                        None => {
                            let tx = tx_opt.take();
                            drop(tx);
                            timer = Instant::now() + FAR_INTERVAL;
                        }
                    }
                }
                conn_ret = rx.recv() => match conn_ret {
                    Some((addr, ret)) => {
                        if let Some(recorder) = request.recorder() {
                            recorder.on_tcp_done(&addr, ret.as_ref().map_err(|e|e.to_string()));
                        }
                        if let Ok(ret) = ret {
                            result = Ok(ret);
                            break 'outer;
                        }
                    }
                    None => {
                        result = Err(crate::Error::AllTcpConnectFailed);
                        break 'outer;
                    },
                }
            }
        }
        _ = cancel.send(());
        result
    }

    pub(crate) async fn tls_handshake(
        &self,
        stream: TcpStream,
        request: &Request,
    ) -> crate::Result<TlsStream<TcpStream>> {
        ensure_crypto_provider();
        if let Some(recorder) = request.recorder() {
            recorder.on_tls_start(&stream);
        }

        let ret = self._tls_handshake(stream, request).await;

        if let Some(recorder) = request.recorder() {
            recorder.on_tls_done(ret.as_ref().map_err(|e| e.to_string()));
        }
        ret
    }

    async fn _dns_resolve(&self, request: &Request) -> crate::Result<(Vec<SocketAddr>, bool)> {
        let host = request.uri().host().ok_or(crate::Error::HostRequired)?;
        let port = request.port();

        if let Some(ips) = self.dns_overrides.get(host) {
            if !ips.is_empty() {
                return Ok((
                    ips.iter().map(|ip| SocketAddr::new(*ip, port)).collect(),
                    true,
                ));
            }
        }

        let ips = tokio::time::timeout(self.dns_timeout, self.resolver.lookup_ip(host)).await??;

        let addrs: Vec<_> = ips
            .into_iter()
            .map(|ip| SocketAddr::new(ip, port))
            .collect();
        if addrs.is_empty() {
            return Err(crate::Error::EmptyResolveResult);
        }
        Ok((addrs, false))
    }

    async fn _tcp_connect(
        local_addr: Option<IpAddr>,
        dest: SocketAddr,
        mut cancel_rx: tokio::sync::broadcast::Receiver<()>,
        prefer_ipv6: bool,
    ) -> crate::Result<TcpStream> {
        let socket = {
            match local_addr {
                Some(local_addr) => match local_addr.is_ipv4() {
                    true => {
                        let socket = TcpSocket::new_v4()?;
                        socket.bind(SocketAddr::new(local_addr, 0))?;
                        socket
                    }
                    false => {
                        let socket = TcpSocket::new_v6()?;
                        socket.bind(SocketAddr::new(local_addr, 0))?;
                        socket
                    }
                },
                None => match prefer_ipv6 {
                    true => TcpSocket::new_v6()?,
                    false => TcpSocket::new_v4()?,
                },
            }
        };

        tokio::select! {
            _ = cancel_rx.recv() => Err(crate::Error::TcpDeadlineExceeded),
            stream = socket.connect(dest) => Ok(stream?),
        }
    }

    async fn _tls_handshake(
        &self,
        stream: TcpStream,
        request: &Request,
    ) -> crate::Result<TlsStream<TcpStream>> {
        // Add root certificates
        let mut root_store = RootCertStore::empty();
        let certs = rustls_native_certs::load_native_certs().certs;
        for cert in certs {
            root_store.add(cert)?;
        }

        // Configure TLS client
        let mut config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        if self.skip_tls_verify {
            config
                .dangerous()
                .set_certificate_verifier(Arc::new(SkipVerifier));
        }

        // Set ALPN protocols
        if let Some(alpn) = self.alpn_protocols.as_ref() {
            config.alpn_protocols = alpn
                .iter()
                .map(|v| v.to_string().as_bytes().to_vec())
                .collect::<Vec<_>>();
        }

        let connector = TlsConnector::from(Arc::new(config));

        let domain = request
            .uri()
            .host()
            .unwrap_or_default()
            .to_string()
            .try_into()?;

        let tls_stream =
            tokio::time::timeout(self.tls_timeout, connector.connect(domain, stream)).await??;

        Ok(tls_stream)
    }

    async fn tcp_send_h1_request(
        &self,
        stream: TcpStream,
        request: Request,
    ) -> crate::Result<Response> {
        let (mut tx, conn) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;

        tokio::spawn(async move {
            _ = conn.await;
        });

        let resp = tx.send_request(request.try_into()?).await?;
        Ok(Response::new(resp.map(super::body::boxed)))
    }

    async fn tls_send_request(
        &self,
        stream: TlsStream<TcpStream>,
        request: Request,
    ) -> crate::Result<Response> {
        let is_h2 = {
            if let Some(alpn) = stream.get_ref().1.alpn_protocol() {
                String::from_utf8_lossy(alpn) == "h2"
            } else {
                false
            }
        };

        let resp = if is_h2 {
            let (mut tx, conn) =
                hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream))
                    .await?;
            tokio::spawn(async move {
                _ = conn.await;
            });
            tx.send_request(request.try_into()?).await?
        } else {
            let (mut tx, conn) =
                hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
            tokio::spawn(async move {
                _ = conn.await;
            });
            tx.send_request(request.try_into()?).await?
        };

        Ok(Response::new(resp.map(super::body::boxed)))
    }
}

#[derive(Debug, Clone)]
pub enum Alpn {
    Http1,
    Http2,
    Http3, // TODO: unsupported yet.
}

impl std::fmt::Display for Alpn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Alpn::Http1 => write!(f, "http/1.1"),
            Alpn::Http2 => write!(f, "h2"),
            Alpn::Http3 => write!(f, "h3"),
        }
    }
}

const FALLBACK_INTERVAL: Duration = Duration::from_secs(3);

const FAR_INTERVAL: Duration = Duration::from_secs(86400 * 365 * 30);

// Initialize crypto provider once
static INIT: Once = Once::new();

fn ensure_crypto_provider() {
    INIT.call_once(|| {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::Instant;

    #[tokio::test]
    async fn test_worker() {
        let mut data = [12, 8, 4, 1].into_iter();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<u64>(1);
        let mut tx_opt = Some(tx);
        let mut next = Instant::now();

        loop {
            tokio::select! {
                _ = tokio::time::sleep_until(next) => {
                    match data.next() {
                        Some(i) => {
                            let tx = tx_opt.clone().unwrap();
                            tokio::spawn(async move {
                                println!("{i} start");
                                tokio::time::sleep(Duration::from_secs(i)).await;
                                _ = tx.send(i).await;
                            });
                            next = Instant::now() + Duration::from_secs(3);
                        }
                        None => {
                            let tx = tx_opt.take().unwrap();
                            drop(tx);
                            // far future
                            next = Instant::now() + Duration::from_secs(86400 * 365 * 30);
                        }
                    }
                }
                v = rx.recv() => match v {
                    Some(i) => println!("{i} done"),
                    None => {
                        println!("everything done");
                        return
                    }
                }
            }
        }
    }
}
