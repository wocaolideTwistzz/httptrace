use std::time::Duration;

use http::{HeaderMap, Method, Request as HttpRequest, Uri, Version};

use crate::{Body, client::Client, stats::Recorder};

#[derive(Default)]
pub struct Request {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Option<Body>,
    timeout: Option<Duration>,
    version: Option<Version>,

    recorder: Option<Box<dyn Recorder>>,
}

pub struct RequestBuilder {
    client: Client,
    request: crate::Result<Request>,
}

impl Request {
    pub fn new(method: Method, uri: Uri) -> Self {
        Self {
            method,
            uri,
            ..Default::default()
        }
    }

    #[inline]
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Get a mutable reference to the method.
    #[inline]
    pub fn method_mut(&mut self) -> &mut Method {
        &mut self.method
    }

    /// Get the url.
    #[inline]
    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    /// Get a mutable reference to the uri.
    #[inline]
    pub fn uri_mut(&mut self) -> &mut Uri {
        &mut self.uri
    }

    /// Get the headers.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Get a mutable reference to the headers.
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        &mut self.headers
    }

    /// Get the body.
    #[inline]
    pub fn body(&self) -> Option<&Body> {
        self.body.as_ref()
    }

    /// Get a mutable reference to the body.
    #[inline]
    pub fn body_mut(&mut self) -> &mut Option<Body> {
        &mut self.body
    }

    /// Get the timeout.
    #[inline]
    pub fn timeout(&self) -> Option<&Duration> {
        self.timeout.as_ref()
    }

    /// Get a mutable reference to the timeout.
    #[inline]
    pub fn timeout_mut(&mut self) -> &mut Option<Duration> {
        &mut self.timeout
    }

    /// Get the http version.
    #[inline]
    pub fn version(&self) -> Option<Version> {
        self.version
    }

    /// Get a mutable reference to the http version.
    #[inline]
    pub fn version_mut(&mut self) -> &mut Option<Version> {
        &mut self.version
    }

    /// Attempt to clone the request.
    ///
    /// `None` is returned if the request can not be cloned, i.e. if the body is a stream.
    pub fn try_clone(&self) -> Option<Request> {
        let body = match self.body.as_ref() {
            Some(body) => Some(body.try_clone()?),
            None => None,
        };
        let mut req = Request::new(self.method().clone(), self.uri().clone());
        *req.timeout_mut() = self.timeout().copied();
        *req.headers_mut() = self.headers().clone();
        *req.version_mut() = self.version();
        req.body = body;
        Some(req)
    }

    pub fn recorder(&self) -> Option<&dyn Recorder> {
        self.recorder.as_deref()
    }

    pub(crate) fn port(&self) -> u16 {
        self.uri.port_u16().unwrap_or_else(|| {
            if self.uri.scheme() == Some(&http::uri::Scheme::HTTPS) {
                443
            } else {
                80
            }
        })
    }
}

impl RequestBuilder {
    pub(super) fn new(client: Client, request: crate::Result<Request>) -> RequestBuilder {
        RequestBuilder { client, request }
    }
}

impl TryFrom<Request> for HttpRequest<Body> {
    type Error = crate::Error;

    fn try_from(value: Request) -> Result<Self, Self::Error> {
        let Request {
            method,
            uri,
            headers,
            body,
            version,
            ..
        } = value;

        let mut builder = HttpRequest::builder();

        if let Some(version) = version {
            builder = builder.version(version);
        }

        let mut req = builder
            .method(method)
            .uri(uri)
            .body(body.unwrap_or_else(Body::empty))?;
        *req.headers_mut() = headers;

        Ok(req)
    }
}
