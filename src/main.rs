use std::{
    iter, str::FromStr, sync::{Arc, LazyLock}, time::SystemTime
};

use anyhow::Context;
use aws_credential_types::{Credentials, provider::ProvideCredentials};
use aws_sigv4::{
    http_request::{SignableBody, SignableRequest, SigningSettings},
    sign::v4,
};
use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    routing::{any, get},
};
use clap::Parser;
use constant_time_eq::constant_time_eq;
use http::{HeaderName, HeaderValue, Method, Request, Response, request, uri::PathAndQuery};
use http_body_util::BodyExt;
use hyper::{
    StatusCode, Uri,
    header::{AUTHORIZATION, HOST},
};
use hyper_tls::HttpsConnector;
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::TokioExecutor,
};
use regex::Regex;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    error::Error,
    options::{LoggingMethod, Options},
};

mod error;
mod options;

static KEY_ID_AS_TOKEN_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("^[A-Z0-9\\-]+ Credential=([^ /,]+)/.*$").unwrap());

#[derive(Debug, Clone)]
pub struct ServerContext {
    pub options: Options,
    pub http_client: Client<HttpsConnector<HttpConnector>, Body>,
    pub aws_config: aws_config::SdkConfig,
    pub aws_credential: Credentials,
    pub token: String,
}

impl ServerContext {
    pub async fn new(options: Options) -> anyhow::Result<Self> {
        // Validate options
        for uri in options
            .upstreams
            .iter()
            .chain(iter::once(&options.endpoint))
        {
            if uri.scheme().is_none() {
                anyhow::bail!("URI must have a scheme: {}", uri);
            }
            if uri.authority().is_none() {
                anyhow::bail!("URI must have an authority: {}", uri);
            }
            if uri.path() != "/" {
                anyhow::bail!("URI's path must be \"/\": {}", uri);
            }
            if uri.query().is_some() {
                anyhow::bail!("URI must not have a query string: {}", uri);
            }
        }
        // HTTP client
        let https = HttpsConnector::new();
        let http_client = Client::builder(TokioExecutor::new()).build(https);
        // AWS config
        let aws_config = aws_config::load_from_env().await;
        let aws_credential = aws_config
            .credentials_provider()
            .context("Failed to get AWS credentials provider")?
            .provide_credentials()
            .await?;
        // Token from environment
        let token = std::env::var("NIX_CACHE_OVERLAY_TOKEN")
            .with_context(|| "Failed to load NIX_CACHE_OVERLAY_TOKEN environment variable")?;
        Ok(ServerContext {
            options,
            http_client,
            aws_config,
            aws_credential,
            token,
        })
    }
}

enum UpstreamState {
    NotFound,
    Found(Response<Body>),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = Options::parse();
    init_tracing_subscriber(&options)?;
    let ctx = Arc::new(ServerContext::new(options).await?);
    let app = Router::new()
        .route("/", get(welcome))
        .route("/{bucket}/{*key}", any(handler))
        .with_state(ctx.clone());
    let listener = tokio::net::TcpListener::bind(&ctx.options.listen[..]).await?;
    tracing::info!("Listening on {:?}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn welcome() -> &'static str {
    "export AWS_ACCESS_KEY_ID=\"$NIX_CACHE_OVERLAY_TOKEN\"
export AWS_SECRET_ACCESS_KEY=\"-\"
export AWS_EC2_METADATA_DISABLED=true
nix copy --to \"s3://$BUCKET_NAME?endpoint=$CACHE_OVERLAY_URL\"
"
}

async fn handler(
    State(ctx): State<Arc<ServerContext>>,
    method: Method,
    Path((_bucket, key)): Path<(String, String)>,
    mut request: Request<Body>,
) -> Result<Response<Body>, Error> {
    tracing::debug!("Handling request: {:?}", request);
    if let Method::GET | Method::HEAD = method {
        // Simply ignore body for upstream check
        let (parts, body) = request.into_parts();
        match check_upstreams(ctx.clone(), &parts, &key).await? {
            UpstreamState::NotFound => {
                // Reconnect body for proxy
                request = Request::from_parts(parts, body);
            }
            UpstreamState::Found(response) => return Ok(response),
        }
    }
    proxy(ctx, request).await
}

async fn check_upstreams(
    ctx: Arc<ServerContext>,
    parts: &request::Parts,
    key: &str,
) -> Result<UpstreamState, Error> {
    for upstream in &ctx.options.upstreams {
        match check_upstream(ctx.clone(), upstream, parts.clone(), key).await? {
            UpstreamState::NotFound => continue,
            result @ UpstreamState::Found(_) => return Ok(result),
        }
    }
    Ok(UpstreamState::NotFound)
}

async fn check_upstream(
    ctx: Arc<ServerContext>,
    upstream: &Uri,
    parts: request::Parts,
    key: &str,
) -> Result<UpstreamState, Error> {
    let mut request = Request::from_parts(parts, Body::empty());
    // Rewrite path to the key
    {
        let uri = request.uri_mut();
        let mut uri_parts = uri.clone().into_parts();
        uri_parts.path_and_query = Some(PathAndQuery::from_str(key)?);
        *uri = Uri::from_parts(uri_parts)?;
    }
    modify_request_to_endpoint(&mut request, upstream)?;
    tracing::info!("Checking upstream: {}", request.uri());
    tracing::trace!("Request to upstream: {:?}", request);
    let response = ctx.http_client.request(request).await?;
    tracing::trace!("Response from upstream: {:?}", response);
    if response.status() == StatusCode::NOT_FOUND {
        Ok(UpstreamState::NotFound)
    } else {
        Ok(UpstreamState::Found(response.map(|incoming| {
            Body::from_stream(incoming.into_data_stream())
        })))
    }
}

async fn proxy(
    ctx: Arc<ServerContext>,
    mut request: Request<Body>,
) -> Result<Response<Body>, Error> {
    // Typical methods are GET, HEAD, PUT, so pad method to 4 chars
    tracing::info!(
        "Forwarding request: {:4} {}",
        request.method(),
        request.uri()
    );
    verify_request(ctx.clone(), &request)?;
    modify_request_to_endpoint(&mut request, &ctx.options.endpoint)?;
    sign_request(ctx.clone(), &mut request)?;
    tracing::debug!("Signed request: {:?}", request);
    let response = ctx.http_client.request(request).await?;
    Ok(response.map(|incoming| Body::from_stream(incoming.into_data_stream())))
}

fn modify_request_to_endpoint(request: &mut Request<Body>, endpoint: &Uri) -> Result<(), Error> {
    let authority = endpoint.authority().expect("endpoint must have authority");

    // Modify Uri
    let uri = request.uri_mut();
    let mut uri_parts = uri.clone().into_parts();
    uri_parts.authority = endpoint.authority().cloned();
    uri_parts.scheme = endpoint.scheme().cloned();
    *uri = Uri::from_parts(uri_parts)?;

    // Modify headers
    let headers = request.headers_mut();
    // Modify Host
    headers.insert(HOST, HeaderValue::from_str(authority.as_str())?);
    // Remove Authorization and X-Forwarded-Host
    headers.remove(AUTHORIZATION);
    headers.remove(HeaderName::from_static("x-forwarded-host"));

    Ok(())
}

fn verify_request(ctx: Arc<ServerContext>, request: &Request<Body>) -> Result<String, Error> {
    let authorization = request
        .headers()
        .get(AUTHORIZATION)
        .ok_or_else(|| Error::NoAuthorization)?
        .to_str()?;
    let token = match KEY_ID_AS_TOKEN_PATTERN.captures(authorization) {
        Some(captures) => captures.get(1).unwrap().as_str(),
        None => return Err(Error::NoTokenInAuthorization(authorization.to_string())),
    };
    if constant_time_eq(token.as_bytes(), ctx.token.as_bytes()) {
        Ok(authorization.to_string())
    } else {
        Err(Error::TokenMismatch)
    }
}

fn sign_request(ctx: Arc<ServerContext>, request: &mut Request<Body>) -> Result<(), Error> {
    // Setting up signing parameters
    let identity = ctx.aws_credential.clone().into();
    let signing_settings = SigningSettings::default();
    let signing_params = v4::SigningParams::builder()
        .identity(&identity)
        .region(&ctx.options.region)
        .name("s3")
        .time(SystemTime::now())
        .settings(signing_settings)
        .build()?
        .into();

    // Convert the HTTP request into a signable request
    let signable_body = match request.headers().get("x-amz-content-sha256") {
        Some(v) => {
            if v == "UNSIGNED-PAYLOAD" {
                SignableBody::UnsignedPayload
            } else {
                SignableBody::Precomputed(v.to_str()?.to_string())
            }
        }
        None => SignableBody::UnsignedPayload,
    };
    let signable_request = SignableRequest::new(
        request.method().as_str(),
        request.uri().to_string(),
        iter::empty(),
        signable_body,
    )?;

    // Sign and then apply the signature to the request
    let (signing_instructions, _signature) =
        aws_sigv4::http_request::sign(signable_request, &signing_params)?.into_parts();
    signing_instructions.apply_to_request_http1x(request);

    Ok(())
}

fn init_tracing_subscriber(options: &Options) -> anyhow::Result<()> {
    let registry =
        tracing_subscriber::registry().with(tracing_subscriber::EnvFilter::from_default_env());
    match options.logging_method {
        LoggingMethod::Console => registry.with(tracing_subscriber::fmt::layer()).try_init()?,
        LoggingMethod::Journald => {
            let journald_layer = tracing_journald::layer()?;
            registry.with(journald_layer).try_init()?;
        }
    }
    Ok(())
}
