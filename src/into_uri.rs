use http::Uri;


/// A trait to try to convert some type into a `Uri`.
///
/// This trait is "sealed", such that only types within rquest can
/// implement it.
pub trait IntoUri: IntoUriSealed {}

impl IntoUri for Uri {}
impl IntoUri for String {}
impl<'a> IntoUri for &'a str {}
impl<'a> IntoUri for &'a String {}

pub trait IntoUriSealed {
    // Besides parsing as a valid `Uri`, the `Uri` must be a valid
    // `http::Uri`, in that it makes sense to use in a network request.
    fn into_uri(self) -> crate::Result<Uri>; 
}

impl IntoUriSealed for Uri {
    fn into_uri(self) -> crate::Result<Uri> {
        if self.host().is_some() {
            Ok(self)
        } else {
            Err(crate::Error::HostRequired)
        }
    }
}

impl<'a> IntoUriSealed for &'a str {
    fn into_uri(self) -> crate::Result<Uri> {
        self.parse::<Uri>()?.into_uri()
    } 
}

impl<'a> IntoUriSealed for &'a String {
    fn into_uri(self) -> crate::Result<Uri> {
        (&**self).into_uri()
    } 
}

impl IntoUriSealed for String {
    fn into_uri(self) -> crate::Result<Uri> {
        (&*self).into_uri()
    } 
}
