use bytes::Bytes;
use encoding_rs::{Encoding, UTF_8};
use http::{HeaderMap, Response as HttpResponse, StatusCode, Version};
use mime::Mime;

use crate::body::ResponseBody;
pub struct Response {
    pub(super) res: HttpResponse<ResponseBody>,
}

impl Response {
    pub(super) fn new(res: HttpResponse<ResponseBody>) -> Self {
        Self { res }
    }

    /// Get the `StatusCode` of this `Response`.
    #[inline]
    pub fn status(&self) -> StatusCode {
        self.res.status()
    }

    /// Get the HTTP `Version` of this `Response`.
    #[inline]
    pub fn version(&self) -> Version {
        self.res.version()
    }

    /// Get the `Headers` of this `Response`.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        self.res.headers()
    }

    /// Get a mutable reference to the `Headers` of this `Response`.
    #[inline]
    pub fn headers_mut(&mut self) -> &mut HeaderMap {
        self.res.headers_mut()
    }

    /// Get the content length of the response, if it is known.
    ///
    /// This value does not directly represents the value of the `Content-Length`
    /// header, but rather the size of the response's body. To read the header's
    /// value, please use the [`Response::headers`] method instead.
    ///
    /// Reasons it may not be known:
    ///
    /// - The response does not include a body (e.g. it responds to a `HEAD`
    ///   request).
    /// - The response is gzipped and automatically decoded (thus changing the
    ///   actual decoded length).
    pub fn content_length(&self) -> Option<u64> {
        use http_body::Body;

        Body::size_hint(self.res.body()).exact()
    }

    pub async fn text(self) -> crate::Result<String> {
        self.text_with_charset("utf-8").await
    }

    pub fn extensions(&self) -> &http::Extensions {
        self.res.extensions()
    }

    /// Returns a mutable reference to the associated extensions.
    pub fn extensions_mut(&mut self) -> &mut http::Extensions {
        self.res.extensions_mut()
    }

    pub async fn text_with_charset(self, default_encoding: &str) -> crate::Result<String> {
        let content_type = self
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<Mime>().ok());
        let encoding_name = content_type
            .as_ref()
            .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()))
            .unwrap_or(default_encoding);
        let encoding = Encoding::for_label(encoding_name.as_bytes()).unwrap_or(UTF_8);

        let full = self.bytes().await?;

        let (text, _, _) = encoding.decode(&full);
        Ok(text.into_owned())
    }

    pub async fn bytes(self) -> crate::Result<Bytes> {
        use http_body_util::BodyExt;

        let d = BodyExt::collect(self.res.into_body())
            .await
            .map(|buf| buf.to_bytes())?;
        Ok(d)
    }

    pub async fn chunk(&mut self) -> crate::Result<Option<Bytes>> {
        use http_body_util::BodyExt;

        // loop to ignore unrecognized frames
        loop {
            if let Some(res) = self.res.body_mut().frame().await {
                let frame = res?;
                if let Ok(buf) = frame.into_data() {
                    return Ok(Some(buf));
                }
                // else continue
            } else {
                return Ok(None);
            }
        }
    }
}
