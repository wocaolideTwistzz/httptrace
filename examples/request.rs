use std::net::SocketAddr;

use hickory_resolver::config::NameServerConfig;
use httptrace::{client::ClientBuilder, request::Request, stats::Recorder};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;

#[tokio::main]
pub async fn main() {
    let client = ClientBuilder::new().build().unwrap();
    let result = client
        .get("https://www.example.com")
        .recorder(Box::new(LogRecorder {}))
        .send()
        .await
        .unwrap();

    println!("https-status: {}", result.status());
    println!("https-body: {:?}", result.text().await);

    let result1 = client
        .get("https://www.example.com")
        .recorder(Box::new(LogRecorder {}))
        .send()
        .await
        .unwrap();

    println!("http-status: {}", result1.status());
    println!("http-body: {:?}", result1.text().await);
}

pub struct LogRecorder {}

impl Recorder for LogRecorder {
    fn on_dns_start(&self, _request: &Request, _name_servers: &[NameServerConfig], _host: &str) {
        println!(
            "{} [dns-start]  {} - {:?}",
            _request.uri(),
            _host,
            _name_servers
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
        );
    }

    fn on_dns_done(
        &self,
        _request: &Request,
        _name_servers: &[NameServerConfig],
        _host: &str,
        _result: Result<(&[SocketAddr], bool), String>,
    ) {
        println!(
            "{} [dns-done]   {} - {:?} --> {:?}",
            _request.uri(),
            _host,
            _name_servers
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>(),
            _result,
        );
    }

    fn on_tcp_start(&self, _request: &Request, _dest: &SocketAddr) {
        println!("{} [tcp-start]  {:?}", _request.uri(), _dest);
    }

    fn on_tcp_done(
        &self,
        _request: &Request,
        _dest: &SocketAddr,
        _stream: Result<&TcpStream, String>,
    ) {
        println!(
            "{} [tcp-done]   {:?} --> {:?}",
            _request.uri(),
            _dest,
            _stream
        );
    }

    fn on_tls_start(&self, _request: &Request, _stream: &TcpStream) {
        println!("{} [tls-start]  {:?}", _request.uri(), _stream.peer_addr());
    }

    fn on_tls_done(&self, _request: &Request, _stream: Result<&TlsStream<TcpStream>, String>) {
        println!(
            "{} [tls-done]   {:?}",
            _request.uri(),
            _stream.map(|stream| {
                let session = stream.get_ref().1;
                format!("protocol: {:?}", session.protocol_version(),)
            })
        );
    }

    fn on_request_start(&self, _request: &Request) {
        println!(
            "{} [request-start] {:?}",
            _request.uri(),
            _request.headers()
        );
    }
}
