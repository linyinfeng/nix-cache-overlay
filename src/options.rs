use std::net::SocketAddr;

use clap::{Parser, ValueEnum};
use http::Uri;

#[derive(Clone, Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct Options {
    #[arg(long, required = true, help = "Listening addresses")]
    pub listen: Vec<SocketAddr>,
    #[arg(
        long,
        help = "Host of upstream cache",
        default_value = "https://cache.nixos.org"
    )]
    pub upstreams: Vec<Uri>,
    #[arg(long, help = "S3 endpoint URL")]
    pub endpoint: Uri,
    #[arg(long, help = "S3 region", default_value = "us-east-1")]
    pub region: String,
    #[arg(long, help = "logging method", default_value = "console")]
    pub logging_method: LoggingMethod,
}

#[derive(Clone, Debug, Copy, ValueEnum)]
pub enum LoggingMethod {
    Console,
    Journald,
}
