use clap::{Parser, ValueEnum};

/// HTTP load testing tool with automatic breaking point detection using oha
#[derive(Parser, Debug)]
#[command(name = "ohabench")]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Target URL(s) to benchmark (can specify multiple)
    #[arg(short, long, action = clap::ArgAction::Append)]
    pub url: Vec<String>,

    /// HTTP method to use
    #[arg(short, long, value_enum, default_value = "get")]
    pub method: HttpMethod,

    /// Request body (for POST, PUT, PATCH)
    #[arg(short, long)]
    pub body: Option<String>,

    /// User-Agent preset or custom string
    #[arg(long, default_value = "ohabench")]
    pub user_agent: String,

    /// Authentication type
    #[arg(long, value_enum, default_value = "none")]
    pub auth_type: AuthType,

    /// Username for basic auth
    #[arg(long)]
    pub auth_user: Option<String>,

    /// Password for basic auth
    #[arg(long)]
    pub auth_pass: Option<String>,

    /// Bearer token for token auth
    #[arg(long)]
    pub auth_token: Option<String>,

    /// Custom auth header (e.g., "X-API-Key: secret")
    #[arg(long)]
    pub auth_header: Option<String>,

    /// Additional headers (repeatable)
    #[arg(short = 'H', long = "header", action = clap::ArgAction::Append)]
    pub headers: Vec<String>,

    /// Ramping mode
    #[arg(long, value_enum, default_value = "linear")]
    pub mode: RampingMode,

    /// Starting rate in requests/second
    #[arg(long, default_value = "50")]
    pub start_rate: u32,

    /// Maximum rate in requests/second
    #[arg(long, default_value = "5000")]
    pub max_rate: u32,

    /// Step size for ramping
    #[arg(long, default_value = "50")]
    pub step: u32,

    /// Duration per step in seconds
    #[arg(short, long, default_value = "30")]
    pub duration: u32,

    /// Number of threads
    #[arg(short, long, default_value = "4")]
    pub threads: u32,

    /// Number of connections
    #[arg(short, long, default_value = "100")]
    pub connections: u32,

    /// Maximum error rate (%) before breaking
    #[arg(long, default_value = "5.0")]
    pub max_error_rate: f64,

    /// Maximum p99 latency (ms) before breaking
    #[arg(long, default_value = "3000")]
    pub max_p99: u32,

    /// Warmup duration in seconds (0 to disable)
    #[arg(long, default_value = "0")]
    pub warmup: u32,

    /// Cooldown between steps in seconds
    #[arg(long, default_value = "0")]
    pub cooldown: u32,

    /// Directory to save report files
    #[arg(short = 'o', long = "output-dir")]
    pub report_dir: Option<String>,

    /// Benchmark name (used for report filenames)
    #[arg(short = 'n', long = "name")]
    pub report_name: Option<String>,

    /// Run in non-interactive mode (requires --url)
    #[arg(long)]
    pub non_interactive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Head => write!(f, "HEAD"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AuthType {
    None,
    Basic,
    Bearer,
    Header,
}

impl std::fmt::Display for AuthType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthType::None => write!(f, "None"),
            AuthType::Basic => write!(f, "Basic Auth"),
            AuthType::Bearer => write!(f, "Bearer Token"),
            AuthType::Header => write!(f, "Custom Header"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum RampingMode {
    Linear,
    Exponential,
}

impl std::fmt::Display for RampingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RampingMode::Linear => write!(f, "Linear"),
            RampingMode::Exponential => write!(f, "Exponential"),
        }
    }
}
