use anyhow::{bail, Context, Result};
use regex::Regex;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

use crate::auth::generate_auth_header;
use crate::config::BenchmarkConfig;

/// Grace period added to benchmark duration before considering it hung (in seconds)
const HANG_TIMEOUT_GRACE_SECONDS: u64 = 30;

/// Results from a single benchmark run
#[derive(Debug, Clone, Default)]
pub struct BenchmarkResult {
    pub target_rate: u32,
    pub actual_rate: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p90_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub max_latency_ms: f64,
    pub total_requests: u64,
    pub errors: u64,
    pub error_rate: f64,
    pub transfer_rate: String,
    /// HTTP status codes that caused errors, sorted by count (descending)
    /// Each tuple is (status_code, count)
    pub error_status_codes: Vec<(u32, u64)>,
    /// Whether the benchmark timed out (server hung)
    pub hung: bool,
}

/// Check if oha is installed
pub fn check_oha_installed() -> Result<()> {
    match Command::new("oha").arg("--version").output() {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                bail!(get_install_instructions())
            }
        }
        Err(_) => {
            bail!(get_install_instructions())
        }
    }
}

fn get_install_instructions() -> String {
    r#"oha is not installed.

oha is a modern HTTP load generator with rate limiting support.

To install oha:

  # Arch Linux
  sudo pacman -S oha

  # Using cargo
  cargo install oha

  # macOS with Homebrew
  brew install oha

For more info, see: https://github.com/hatoo/oha"#
        .to_string()
}

/// Build the oha command for a benchmark run
fn build_oha_command(config: &BenchmarkConfig, url: &str, rate: u32) -> Command {
    let mut cmd = Command::new("oha");

    // Basic options
    cmd.arg("-c").arg(config.ramping.connections.to_string());
    cmd.arg("-z")
        .arg(format!("{}s", config.ramping.duration_seconds));
    cmd.arg("-q").arg(rate.to_string());
    cmd.arg("--latency-correction"); // Fix coordinated omission
    cmd.arg("-w"); // Wait for ongoing requests after deadline (prevents false errors)
    cmd.arg("--no-tui"); // Disable TUI for scripting

    // HTTP method
    cmd.arg("-m").arg(config.method.to_string().to_uppercase());

    // Request body
    if let Some(ref body) = config.body {
        cmd.arg("-d").arg(body);
        // Add Content-Type if not already specified
        if !config
            .headers
            .iter()
            .any(|h| h.to_lowercase().starts_with("content-type:"))
        {
            cmd.arg("-H").arg("Content-Type: application/json");
        }
    }

    // User-Agent header
    cmd.arg("-H")
        .arg(format!("User-Agent: {}", config.user_agent));

    // Auth header
    if let Some(auth_header) = generate_auth_header(&config.auth) {
        cmd.arg("-H").arg(auth_header);
    }

    // Additional headers
    for header in &config.headers {
        cmd.arg("-H").arg(header);
    }

    // URL
    cmd.arg(url);

    cmd
}

/// Run a single benchmark at the specified rate
pub fn run_benchmark(config: &BenchmarkConfig, url: &str, rate: u32) -> Result<BenchmarkResult> {
    let mut cmd = build_oha_command(config, url, rate);

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().context("Failed to spawn oha process")?;

    // Calculate timeout: benchmark duration + grace period
    let timeout_secs = config.ramping.duration_seconds as u64 + HANG_TIMEOUT_GRACE_SECONDS;
    let timeout = Duration::from_secs(timeout_secs);

    // Wait with timeout
    match child
        .wait_timeout(timeout)
        .context("Failed to wait for oha")?
    {
        Some(_status) => {
            // Process completed within timeout
            let mut stdout = String::new();
            let mut stderr = String::new();

            if let Some(mut stdout_pipe) = child.stdout.take() {
                stdout_pipe.read_to_string(&mut stdout).ok();
            }
            if let Some(mut stderr_pipe) = child.stderr.take() {
                stderr_pipe.read_to_string(&mut stderr).ok();
            }

            // Combine stdout and stderr for parsing
            let full_output = format!("{}\n{}", stdout, stderr);

            parse_oha_output(&full_output, rate, config.ramping.duration_seconds)
        }
        None => {
            // Timeout - process hung, kill it
            child.kill().ok();
            child.wait().ok(); // Reap the zombie

            // Return a hung result
            Ok(BenchmarkResult {
                target_rate: rate,
                hung: true,
                error_rate: 100.0,
                ..Default::default()
            })
        }
    }
}

/// Run a warmup period
pub fn run_warmup(config: &BenchmarkConfig, url: &str) -> Result<()> {
    if config.warmup_seconds == 0 {
        return Ok(());
    }

    let mut cmd = Command::new("oha");
    cmd.arg("-c").arg(config.ramping.connections.to_string());
    cmd.arg("-z").arg(format!("{}s", config.warmup_seconds));
    cmd.arg("-q").arg(config.ramping.start_rate.to_string());
    cmd.arg("--no-tui");

    // Add headers
    cmd.arg("-H")
        .arg(format!("User-Agent: {}", config.user_agent));
    if let Some(auth_header) = generate_auth_header(&config.auth) {
        cmd.arg("-H").arg(auth_header);
    }

    cmd.arg(url);

    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());

    let status = cmd.status().context("Failed to run warmup")?;
    if !status.success() {
        bail!("Warmup failed");
    }

    Ok(())
}

/// Parse oha output into a BenchmarkResult
///
/// Example oha output:
/// ```
/// Summary:
///   Success rate: 100.00%
///   Total:        3005.1945 ms
///   Slowest:      776.2771 ms
///   Fastest:      142.7181 ms
///   Average:      239.4548 ms
///   Requests/sec: 9.9827
///
/// Response time distribution:
///   10.00% in 157.9695 ms
///   25.00% in 172.4477 ms
///   50.00% in 196.0308 ms
///   75.00% in 262.3140 ms
///   90.00% in 378.1813 ms
///   95.00% in 466.6556 ms
///   99.00% in 776.2771 ms
///
/// Status code distribution:
///   [200] 28 responses
///
/// Error distribution:
///   [2] aborted due to deadline
/// ```
fn parse_oha_output(
    output: &str,
    target_rate: u32,
    duration_seconds: u32,
) -> Result<BenchmarkResult> {
    let mut result = BenchmarkResult {
        target_rate,
        ..Default::default()
    };

    // Parse success rate (to calculate error rate)
    let success_rate_re = Regex::new(r"Success rate:\s+([\d.]+)%")?;
    if let Some(caps) = success_rate_re.captures(output) {
        let success_rate: f64 = caps[1].parse().unwrap_or(100.0);
        result.error_rate = 100.0 - success_rate;
    }

    // Parse average latency
    let avg_re = Regex::new(r"Average:\s+([\d.]+)\s*(us|ms|s|m)")?;
    if let Some(caps) = avg_re.captures(output) {
        result.avg_latency_ms = parse_time_to_ms(&caps[1], &caps[2]);
    }

    // Parse slowest (max) latency
    let slowest_re = Regex::new(r"Slowest:\s+([\d.]+)\s*(us|ms|s|m)")?;
    if let Some(caps) = slowest_re.captures(output) {
        result.max_latency_ms = parse_time_to_ms(&caps[1], &caps[2]);
    }

    // Parse requests/sec
    let rps_re = Regex::new(r"Requests/sec:\s+([\d.]+)")?;
    if let Some(caps) = rps_re.captures(output) {
        result.actual_rate = caps[1].parse().unwrap_or(0.0);
    }

    // Parse percentiles from "Response time distribution"
    let p50_re = Regex::new(r"50\.00%\s+in\s+([\d.]+)\s*(us|ms|s|m)")?;
    let p90_re = Regex::new(r"90\.00%\s+in\s+([\d.]+)\s*(us|ms|s|m)")?;
    let p99_re = Regex::new(r"99\.00%\s+in\s+([\d.]+)\s*(us|ms|s|m)")?;

    if let Some(caps) = p50_re.captures(output) {
        result.p50_latency_ms = parse_time_to_ms(&caps[1], &caps[2]);
    }
    if let Some(caps) = p90_re.captures(output) {
        result.p90_latency_ms = parse_time_to_ms(&caps[1], &caps[2]);
    }
    if let Some(caps) = p99_re.captures(output) {
        result.p99_latency_ms = parse_time_to_ms(&caps[1], &caps[2]);
    }

    // Parse status code responses to get total requests
    // Matches: [200] 28 responses, [404] 5 responses, etc.
    let status_re = Regex::new(r"\[(\d+)\]\s+(\d+)\s+responses?")?;
    let mut total_requests: u64 = 0;
    let mut error_responses: u64 = 0;
    let mut error_status_codes: Vec<(u32, u64)> = Vec::new();

    for caps in status_re.captures_iter(output) {
        let status_code: u32 = caps[1].parse().unwrap_or(0);
        let count: u64 = caps[2].parse().unwrap_or(0);
        total_requests += count;

        // Count non-2xx/3xx as errors
        if !(200..400).contains(&status_code) {
            error_responses += count;
            error_status_codes.push((status_code, count));
        }
    }

    // Sort by count descending to get most common errors first
    error_status_codes.sort_by(|a, b| b.1.cmp(&a.1));

    // Parse error distribution for additional errors (connection errors, timeouts, etc.)
    // Matches: [2] aborted due to deadline, [1] connection refused, etc.
    let error_re = Regex::new(r"Error distribution:\s*\n((?:\s+\[\d+\][^\n]+\n?)+)")?;
    if let Some(caps) = error_re.captures(output) {
        let error_section = &caps[1];
        let error_count_re = Regex::new(r"\[(\d+)\]")?;
        for err_caps in error_count_re.captures_iter(error_section) {
            let count: u64 = err_caps[1].parse().unwrap_or(0);
            result.errors += count;
            // Connection errors are attempted requests that never got a response
            total_requests += count;
        }
    }

    result.errors += error_responses;
    result.total_requests = total_requests;
    result.error_status_codes = error_status_codes;

    // Recalculate error rate if we have total requests
    if result.total_requests > 0 && result.errors > 0 {
        result.error_rate = (result.errors as f64 / result.total_requests as f64) * 100.0;
    }

    // Parse transfer rate (Size/sec)
    let transfer_re = Regex::new(r"Size/sec:\s+(.+)")?;
    if let Some(caps) = transfer_re.captures(output) {
        result.transfer_rate = caps[1].trim().to_string();
    }

    // Estimate total requests from rate * duration if not parsed
    if result.total_requests == 0 && result.actual_rate > 0.0 {
        result.total_requests = (result.actual_rate * duration_seconds as f64) as u64;
    }

    Ok(result)
}

/// Convert time value to milliseconds
fn parse_time_to_ms(value: &str, unit: &str) -> f64 {
    let v: f64 = value.parse().unwrap_or(0.0);
    match unit {
        "us" => v / 1000.0,
        "ms" => v,
        "s" => v * 1000.0,
        "m" => v * 60_000.0,
        _ => v,
    }
}
