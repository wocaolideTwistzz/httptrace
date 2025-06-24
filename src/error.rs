use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("uri parse error {0}")]
    Uri(#[from] http::uri::InvalidUri),

    #[error("resolve error {0}")]
    Resolve(#[from] hickory_resolver::ResolveError),

    #[error("{category}: {message}")]
    Common { category: String, message: String },

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
}

pub type Result<T> = std::result::Result<T, Error>;
