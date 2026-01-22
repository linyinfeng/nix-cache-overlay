use axum::{
    body::Body,
    http::{self, Response, StatusCode},
    response::IntoResponse,
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    // Server side errors
    #[error("http error: {0}")]
    Http(#[from] http::Error),
    #[error("hyper client error: {0}")]
    HyperClient(#[from] hyper_util::client::legacy::Error),
    #[error("invalid header value: {0}")]
    InvalidHeaderValue(#[from] http::header::InvalidHeaderValue),
    #[error("signing error: {0}")]
    Signing(#[from] aws_sigv4::http_request::SigningError),
    #[error("invalid uri parts: {0}")]
    InvalidUriParts(#[from] http::uri::InvalidUriParts),

    // Client side errors
    #[error("token mismatch")]
    TokenMismatch,
    #[error("no authorization header found")]
    NoAuthorization,
    #[error("invalid header: {0}")]
    ToStrError(#[from] http::header::ToStrError),
    #[error("no token in authorization header: {0}")]
    NoTokenInAuthorization(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response<Body> {
        tracing::info!("report error: {self}");
        tracing::debug!("            : {self:?}");
        let code = self.code();
        let body = if code.is_client_error() {
            self.to_string()
        } else {
            code.canonical_reason()
                .unwrap_or("unknown error")
                .to_owned()
        };
        (code, body).into_response()
    }
}

impl Error {
    pub fn code(&self) -> StatusCode {
        match self {
            // Server side errors
            Error::Http(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::HyperClient(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::InvalidHeaderValue(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::Signing(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Error::InvalidUriParts(_) => StatusCode::INTERNAL_SERVER_ERROR,
            // Client side errors
            Error::TokenMismatch => StatusCode::FORBIDDEN,
            Error::NoAuthorization => StatusCode::FORBIDDEN,
            Error::ToStrError(_) => StatusCode::BAD_REQUEST,
            Error::NoTokenInAuthorization(_) => StatusCode::BAD_REQUEST,
        }
    }
}
