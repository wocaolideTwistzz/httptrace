use std::task::{Poll, ready};
use std::{pin::Pin, time::Duration};

use bytes::Bytes;
use http_body::Body as HttpBody;
use http_body::Frame;
use http_body_util::{StreamBody, combinators::BoxBody};
use pin_project_lite::pin_project;
use tokio::time::Sleep;

/// An asynchronous request body
pub struct Body {
    inner: Inner,
}

enum Inner {
    Reuseable(Bytes),
    Streaming(BoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>),
}

pin_project! {
    ///  A body with a total timeout
    ///
    /// The timeout does not reset upon each chunk, but rather requires the whole
    /// body be streamed before the deadline is reached.
    pub(crate) struct TotalTimeoutBody<B> {
        #[pin]
        inner:B,
        timeout: Pin<Box<Sleep>>
    }
}

pin_project! {
    pub(crate) struct ReadTimeoutBody<B> {
        #[pin]
        inner: B,
        #[pin]
        sleep: Option<Sleep>,
        timeout: Duration
    }
}

pub(crate) struct DataStream<B>(pub(crate) B);

impl Body {
    /// Returns a reference to the internal data of the `Body`.
    ///
    /// `None` is returned, if the underlying  data is a stream.
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match &self.inner {
            Inner::Reuseable(bytes) => Some(bytes),
            Inner::Streaming(..) => None,
        }
    }

    /// Wrap a futures `Stream` in a box inside [`Body`].
    ///
    /// # Example
    /// ```
    /// use httptrace::Body;
    /// use futures_util;
    ///
    /// let chunks: Vec<Result<_, ::std::io::Error>> = vec![
    ///     Ok("hello"),
    ///     Ok(" "),
    ///     Ok("world"),
    /// ];
    ///
    /// let stream = futures_util::stream::iter(chunks);
    ///
    /// let _body = Body::wrap_stream(stream);
    /// ```
    pub fn wrap_stream<S>(stream: S) -> Body
    where
        S: futures_util::stream::TryStream + Send + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        Bytes: From<S::Ok>,
    {
        Body::stream(stream)
    }

    pub(crate) fn stream<S>(stream: S) -> Body
    where
        S: futures_util::stream::TryStream + Send + 'static,
        S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
        Bytes: From<S::Ok>,
    {
        use futures_util::stream::TryStreamExt;
        let body = http_body_util::BodyExt::boxed(StreamBody::new(sync_wrapper::SyncStream::new(
            stream
                .map_ok(|d| Frame::data(Bytes::from(d)))
                .map_err(Into::into),
        )));
        Body {
            inner: Inner::Streaming(body),
        }
    }

    pub(crate) fn empty() -> Body {
        Body::reuseable(Bytes::new())
    }

    pub(crate) fn reuseable(chunk: Bytes) -> Body {
        Body {
            inner: Inner::Reuseable(chunk),
        }
    }

    /// Wrap a [`HttpBody`] in a box inside [`Body`]
    ///
    /// # Example
    /// ```
    /// use httptrace::Body;
    /// use futures_util;
    ///
    /// let content = "hello_world".to_string();
    ///
    /// let _body = Body::wrap(content);
    /// ```
    pub fn wrap<B>(inner: B) -> Body
    where
        B: HttpBody + Send + Sync + 'static,
        B::Data: Into<Bytes>,
        B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    {
        use http_body_util::BodyExt;

        let boxed = IntoBytesBody { inner }.map_err(Into::into).boxed();

        Body {
            inner: Inner::Streaming(boxed),
        }
    }

    pub(crate) fn try_reuse(self) -> (Option<Bytes>, Self) {
        let reuse = match self.inner {
            Inner::Reuseable(ref chunk) => Some(chunk.clone()),
            Inner::Streaming(..) => None,
        };

        (reuse, self)
    }

    pub(crate) fn try_clone(&self) -> Option<Body> {
        match self.inner {
            Inner::Reuseable(ref chunk) => Some(Body::reuseable(chunk.clone())),
            Inner::Streaming(..) => None,
        }
    }

    pub(crate) fn into_stream(self) -> DataStream<Body> {
        DataStream(self)
    }

    pub(crate) fn content_length(&self) -> Option<u64> {
        match self.inner {
            Inner::Reuseable(ref bytes) => Some(bytes.len() as u64),
            Inner::Streaming(ref body) => body.size_hint().exact(),
        }
    }
}

impl Default for Body {
    fn default() -> Self {
        Body::empty()
    }
}

// ======= impl IntoBytesBody ========
pin_project! {
    struct IntoBytesBody<B> {
        #[pin]
        inner: B
    }
}

impl<B> HttpBody for IntoBytesBody<B>
where
    B: HttpBody,
    B::Data: Into<Bytes>,
{
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match ready!(self.project().inner.poll_frame(cx)) {
            Some(Ok(f)) => Poll::Ready(Some(Ok(f.map_data(Into::into)))),
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => Poll::Ready(None),
        }
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.inner.size_hint()
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }
}
