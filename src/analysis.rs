use std::collections::HashMap;

use crate::config::ThresholdConfig;
use crate::runner::BenchmarkResult;

/// Status of a benchmark step
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Ok,
    Warning,
    Break,
    RateLimited,
    Blocked,
    Hung,
    Gone,
}

impl std::fmt::Display for StepStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepStatus::Ok => write!(f, "OK"),
            StepStatus::Warning => write!(f, "WARN"),
            StepStatus::Break => write!(f, "BREAK"),
            StepStatus::RateLimited => write!(f, "RATE"),
            StepStatus::Blocked => write!(f, "BLOCK"),
            StepStatus::Hung => write!(f, "HANG"),
            StepStatus::Gone => write!(f, "GONE"),
        }
    }
}

/// Reason for breaking
#[derive(Debug, Clone)]
pub enum BreakReason {
    ErrorRate(f64),
    RateLimited(f64), // Error rate when rate limited (429)
    Blocked(f64),     // Error rate when blocked by WAF/security (403)
    P99Latency(f64),
    ThroughputDegradation(f64), // actual rate vs target rate percentage
    Hung,                       // Server stopped responding
    NoResponses,                // No successful responses received
    None,
}

impl std::fmt::Display for BreakReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BreakReason::ErrorRate(rate) => {
                write!(f, "Error rate exceeded threshold ({:.1}%)", rate)
            }
            BreakReason::RateLimited(rate) => {
                write!(f, "Rate limited by server ({:.1}% errors)", rate)
            }
            BreakReason::Blocked(rate) => {
                write!(f, "Blocked by WAF/security ({:.1}% errors)", rate)
            }
            BreakReason::P99Latency(ms) => {
                write!(f, "p99 latency exceeded threshold ({:.0}ms)", ms)
            }
            BreakReason::ThroughputDegradation(pct) => {
                write!(f, "Throughput degradation ({:.1}% of target)", pct)
            }
            BreakReason::Hung => write!(f, "Server stopped responding"),
            BreakReason::NoResponses => write!(f, "No successful responses received"),
            BreakReason::None => write!(f, ""),
        }
    }
}

/// Analysis of benchmark results
#[derive(Debug)]
pub struct AnalysisResult {
    pub status: StepStatus,
    pub break_reason: BreakReason,
}

/// Summary of all benchmark results
#[derive(Debug)]
pub struct BenchmarkSummary {
    pub breaking_point_rate: Option<u32>,
    pub break_reason: BreakReason,
    pub last_stable_rate: Option<u32>,
    pub recommended_rate: Option<u32>,
    pub total_requests: u64,
    pub total_duration_seconds: u64,
    pub was_rate_limited: bool,
    pub was_blocked: bool,
    /// Aggregated HTTP error status codes across all results, sorted by count (descending)
    pub aggregated_error_codes: Vec<(u32, u64)>,
}

/// Generate a summary from all benchmark results
pub fn generate_summary(
    results: &[BenchmarkResult],
    analyses: &[AnalysisResult],
    duration_per_step: u32,
) -> BenchmarkSummary {
    let mut breaking_point_rate: Option<u32> = None;
    let mut break_reason = BreakReason::None;
    let mut last_stable_rate: Option<u32> = None;
    let mut was_rate_limited = false;
    let mut was_blocked = false;

    for (i, analysis) in analyses.iter().enumerate() {
        let is_terminal = matches!(
            analysis.status,
            StepStatus::Break
                | StepStatus::RateLimited
                | StepStatus::Blocked
                | StepStatus::Hung
                | StepStatus::Gone
        );

        if is_terminal {
            breaking_point_rate = Some(results[i].target_rate);
            break_reason = analysis.break_reason.clone();
            was_rate_limited = analysis.status == StepStatus::RateLimited;
            was_blocked = analysis.status == StepStatus::Blocked;
            if i > 0 {
                last_stable_rate = Some(results[i - 1].target_rate);
            }
            break;
        } else if analysis.status == StepStatus::Ok {
            last_stable_rate = Some(results[i].target_rate);
        }
    }

    // If we never broke, last stable is the last rate tested
    if breaking_point_rate.is_none() && !results.is_empty() {
        last_stable_rate = Some(results.last().unwrap().target_rate);
    }

    // Recommended rate is 80% of last stable
    let recommended_rate = last_stable_rate.map(|r| (r as f64 * 0.8) as u32);

    // Total requests and duration
    let total_requests: u64 = results.iter().map(|r| r.total_requests).sum();
    let total_duration_seconds = (results.len() as u64) * (duration_per_step as u64);

    // Aggregate error codes across all results
    let mut error_code_counts: HashMap<u32, u64> = HashMap::new();
    for result in results {
        for (code, count) in &result.error_status_codes {
            *error_code_counts.entry(*code).or_insert(0) += count;
        }
    }
    // Convert to sorted vec (by count descending)
    let mut aggregated_error_codes: Vec<(u32, u64)> = error_code_counts.into_iter().collect();
    aggregated_error_codes.sort_by(|a, b| b.1.cmp(&a.1));

    BenchmarkSummary {
        breaking_point_rate,
        break_reason,
        last_stable_rate,
        recommended_rate,
        total_requests,
        total_duration_seconds,
        was_rate_limited,
        was_blocked,
        aggregated_error_codes,
    }
}

/// Returns the most common (plurality) HTTP error status code, if any errors exist
/// Uses plurality - whichever error code has the highest count wins
fn get_dominant_error_status(result: &BenchmarkResult) -> Option<u32> {
    // error_status_codes is already sorted by count descending, so first item is most common
    result.error_status_codes.first().map(|(code, _)| *code)
}

/// Analyze a single benchmark result against thresholds
pub fn analyze_result(result: &BenchmarkResult, thresholds: &ThresholdConfig) -> AnalysisResult {
    // Check if benchmark hung (timed out)
    if result.hung {
        return AnalysisResult {
            status: StepStatus::Hung,
            break_reason: BreakReason::Hung,
        };
    }

    // Check if no successful responses were received (all latency values are 0)
    // This indicates complete failure - server didn't respond to any requests
    if result.p99_latency_ms == 0.0 && result.avg_latency_ms == 0.0 && result.actual_rate > 0.0 {
        return AnalysisResult {
            status: StepStatus::Gone,
            break_reason: BreakReason::NoResponses,
        };
    }

    // Check error rate
    if result.error_rate > thresholds.max_error_rate {
        // Use plurality: whichever error code is most common determines the status
        match get_dominant_error_status(result) {
            Some(429) => {
                return AnalysisResult {
                    status: StepStatus::RateLimited,
                    break_reason: BreakReason::RateLimited(result.error_rate),
                };
            }
            Some(403) => {
                return AnalysisResult {
                    status: StepStatus::Blocked,
                    break_reason: BreakReason::Blocked(result.error_rate),
                };
            }
            _ => {
                return AnalysisResult {
                    status: StepStatus::Break,
                    break_reason: BreakReason::ErrorRate(result.error_rate),
                };
            }
        }
    }

    // Check p99 latency
    if result.p99_latency_ms > thresholds.max_p99_ms as f64 {
        return AnalysisResult {
            status: StepStatus::Break,
            break_reason: BreakReason::P99Latency(result.p99_latency_ms),
        };
    }

    // Check throughput degradation (if actual is less than 70% of target)
    let target = result.target_rate as f64;
    let actual_pct = (result.actual_rate / target) * 100.0;
    if actual_pct < 70.0 && result.target_rate > 0 {
        return AnalysisResult {
            status: StepStatus::Break,
            break_reason: BreakReason::ThroughputDegradation(actual_pct),
        };
    }

    // Check for warnings (approaching thresholds)
    let error_warning = result.error_rate > thresholds.max_error_rate * 0.5;
    let latency_warning = result.p99_latency_ms > thresholds.max_p99_ms as f64 * 0.7;

    if error_warning || latency_warning {
        return AnalysisResult {
            status: StepStatus::Warning,
            break_reason: BreakReason::None,
        };
    }

    AnalysisResult {
        status: StepStatus::Ok,
        break_reason: BreakReason::None,
    }
}
