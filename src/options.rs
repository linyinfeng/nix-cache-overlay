use std::net::SocketAddr;

use clap::Parser;
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
}
