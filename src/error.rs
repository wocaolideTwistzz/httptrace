use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("unknown error")]
    Unknown,

    #[error("uri parse error {0}")]
    Uri(#[from] http::uri::InvalidUri),

    #[error("resolve error {0}")]
    Resolve(#[from] hickory_resolver::ResolveError),

    #[error("io error {0}")]
    Io(#[from] std::io::Error),

    #[error("timeout error {0}")]
    Timeout(#[from] tokio::time::error::Elapsed),

    #[error("rustls error {0}")]
    Rustls(#[from] tokio_rustls::rustls::Error),

    #[error("invalid dns name error {0}")]
    InvalidDnsName(#[from] tokio_rustls::rustls::pki_types::InvalidDnsNameError),

    #[error("hyper error {0}")]
    Hyper(#[from] hyper::Error),

    #[error("http error {0}")]
    Http(#[from] http::Error),

    #[error("http invalid header value {0}")]
    HttpInvalidHeader(#[from] http::header::InvalidHeaderValue),

    #[error("host required")]
    HostRequired,

    #[error("empty resolve result")]
    EmptyResolveResult,

    #[error("all tcp connect failed")]
    AllTcpConnectFailed,

    #[error("tcp deadline exceeded")]
    TcpDeadlineExceeded,

    #[error("body error: {0}")]
    Body(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("body timeout")]
    BodyTimeout,
}

pub type Result<T> = std::result::Result<T, Error>;
