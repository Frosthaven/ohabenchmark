use console::style;
use std::fmt::Write as FmtWrite;
use std::io::Write;

use crate::analysis::{AnalysisResult, BenchmarkSummary, StepStatus};
use crate::config::BenchmarkConfig;
use crate::runner::BenchmarkResult;

const SEPARATOR: &str =
    "═══════════════════════════════════════════════════════════════════════════════";

/// Print the application header
pub fn print_header() {
    println!("{}", style(SEPARATOR).dim());
    println!(
        "{}",
        style("ohabench - HTTP Load Testing with Breaking Point Detection").bold()
    );
    println!("{}", style(SEPARATOR).dim());
}

/// Print benchmark configuration summary
pub fn print_config_summary(config: &BenchmarkConfig) {
    if config.urls.len() == 1 {
        println!("{:<14} {}", style("Target:").cyan(), config.urls[0]);
    } else {
        println!(
            "{:<14} {} URLs",
            style("Targets:").cyan(),
            config.urls.len()
        );
        for (i, url) in config.urls.iter().enumerate() {
            println!("{:<14} {}. {}", "", i + 1, url);
        }
    }
    println!("{:<14} {}", style("Method:").cyan(), config.method);
    println!(
        "{:<14} {} ramping",
        style("Mode:").cyan(),
        config.ramping.mode
    );
    println!(
        "{:<14} {} -> {} req/s",
        style("Range:").cyan(),
        config.ramping.start_rate,
        config.ramping.max_rate
    );
    println!(
        "{:<14} {}s per step",
        style("Duration:").cyan(),
        config.ramping.duration_seconds
    );
    println!(
        "{:<14} Error rate > {}% OR p99 > {}ms",
        style("Break when:").cyan(),
        config.thresholds.max_error_rate,
        config.thresholds.max_p99_ms
    );
    println!("{}", style(SEPARATOR).dim());
}

/// Print URL header for multi-URL runs
pub fn print_url_header(url: &str, index: usize, total: usize) {
    println!();
    println!("{}", style(SEPARATOR).dim());
    println!(
        "{} [{}/{}] {}",
        style("BENCHMARKING").cyan().bold(),
        index + 1,
        total,
        style(url).bold()
    );
    println!("{}", style(SEPARATOR).dim());
}

/// Print the table header
pub fn print_table_header() {
    println!();
    println!(
        "{:>7} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8} {:>9} {:>7}",
        "Target", "Actual", "Avg Lat", "p50", "p90", "p99", "Max", "Err Rate", "Status"
    );
    println!(
        "{:>7} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8} {:>9} {:>7}",
        "───────",
        "─────────",
        "─────────",
        "────────",
        "────────",
        "────────",
        "────────",
        "─────────",
        "───────"
    );
}

/// Format latency value with appropriate unit
fn format_latency(ms: f64) -> String {
    if ms == 0.0 {
        "-".to_string()
    } else if ms < 1.0 {
        format!("{:.0}us", ms * 1000.0)
    } else if ms < 1000.0 {
        format!("{:.0}ms", ms)
    } else {
        format!("{:.1}s", ms / 1000.0)
    }
}

/// Print a single result row
pub fn print_result_row(result: &BenchmarkResult, analysis: &AnalysisResult) {
    // Build status string (no error codes in table - they go in summary)
    let status_text = match analysis.status {
        StepStatus::Ok => "OK",
        StepStatus::Warning => "WARN",
        StepStatus::Break => "BREAK",
        StepStatus::RateLimited => "RATE",
        StepStatus::Blocked => "BLOCK",
        StepStatus::Hung => "HANG",
    };

    let status_padded = format!("{:>6}", status_text);

    let status_str = match analysis.status {
        StepStatus::Ok => style(status_padded).green().to_string(),
        StepStatus::Warning => style(status_padded).yellow().to_string(),
        StepStatus::Break | StepStatus::RateLimited | StepStatus::Blocked | StepStatus::Hung => {
            style(status_padded).red().bold().to_string()
        }
    };

    let error_rate_padded = format!("{:>8.2}%", result.error_rate);
    let error_rate_str = if result.error_rate > 0.0 {
        style(error_rate_padded).red().to_string()
    } else {
        error_rate_padded
    };

    println!(
        "{:>7} {:>9.1} {:>9} {:>8} {:>8} {:>8} {:>8} {} {}",
        result.target_rate,
        result.actual_rate,
        format_latency(result.avg_latency_ms),
        format_latency(result.p50_latency_ms),
        format_latency(result.p90_latency_ms),
        format_latency(result.p99_latency_ms),
        format_latency(result.max_latency_ms),
        error_rate_str,
        status_str
    );
}

/// Print the results summary
pub fn print_summary(summary: &BenchmarkSummary) {
    println!();
    println!("{}", style(SEPARATOR).dim());
    println!("{}", style("RESULTS").bold());
    println!("{}", style(SEPARATOR).dim());

    if let Some(rate) = summary.breaking_point_rate {
        // Choose appropriate label based on what caused the stop
        let label = if summary.was_rate_limited {
            "Rate limited at:"
        } else if summary.was_blocked {
            "Blocked at:"
        } else {
            "Breaking point:"
        };

        println!(
            "{:<22} {} req/s ({})",
            style(label).cyan(),
            style(rate).red(),
            summary.break_reason
        );
    } else {
        println!(
            "{:<22} {}",
            style("Breaking point:").cyan(),
            style("Not reached (consider increasing max rate)").green()
        );
    }

    // Display aggregated HTTP error codes if any
    if !summary.aggregated_error_codes.is_empty() {
        let error_codes_str: Vec<String> = summary
            .aggregated_error_codes
            .iter()
            .map(|(code, count)| format!("{} ({})", code, format_number(*count)))
            .collect();
        println!(
            "{:<22} {}",
            style("HTTP errors:").cyan(),
            error_codes_str.join(", ")
        );
    }

    if let Some(rate) = summary.last_stable_rate {
        println!(
            "{:<22} {} req/s",
            style("Last stable rate:").cyan(),
            style(rate).green()
        );
    }

    if let Some(rate) = summary.recommended_rate {
        println!(
            "{:<22} {} req/s (80% of last stable)",
            style("Recommended rate:").cyan(),
            style(rate).green().bold()
        );
    }

    println!(
        "{:<22} ~{}",
        style("Total requests:").cyan(),
        format_number(summary.total_requests)
    );

    println!(
        "{:<22} {}",
        style("Total duration:").cyan(),
        format_duration(summary.total_duration_seconds)
    );
}

/// Print the legend
pub fn print_legend() {
    println!();
    println!("{}", style(SEPARATOR).dim());
    println!("{}", style("LEGEND").bold());
    println!("{}", style(SEPARATOR).dim());
    println!(
        "{:<12} Requested rate in requests/second",
        style("Target").cyan()
    );
    println!(
        "{:<12} Achieved throughput (lower than target = saturation)",
        style("Actual").cyan()
    );
    println!("{:<12} Mean response latency", style("Avg Lat").cyan());
    println!(
        "{:<12} Latency percentiles (50% / 90% / 99% of requests faster than this)",
        style("p50/p90/p99").cyan()
    );
    println!("{:<12} Maximum observed latency", style("Max").cyan());
    println!(
        "{:<12} Non-2xx responses + connection/timeout errors as percentage",
        style("Err Rate").cyan()
    );
    println!();
    println!("{}", style("Status Codes:").cyan().bold());
    println!(
        "  {} = under threshold    {} = approaching threshold",
        style("OK").green(),
        style("WARN").yellow()
    );
    println!(
        "  {} = server breaking    {} = rate limited (429)   {} = blocked (403)",
        style("BREAK").red(),
        style("RATE").red(),
        style("BLOCK").red()
    );
    println!("  {} = server hung (timed out)", style("HANG").red());
    println!();
    println!(
        "{}",
        style("Expected Error Rates by Service Type:")
            .yellow()
            .bold()
    );
    println!("  {:<24} {}", "Payment/Checkout:", style("< 0.1%").green());
    println!(
        "  {:<24} {}",
        "Core App Functionality:",
        style("< 0.5%").green()
    );
    println!("  {:<24} {}", "APIs:", style("< 1%").green());
    println!(
        "  {:<24} {}",
        "Non-critical Features:",
        style("< 2%").yellow()
    );
    println!();
    println!("Recommended rate is 80% of last stable rate for safety margin.");
    println!("{}", style(SEPARATOR).dim());
}

/// Format a large number with commas
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format duration in human-readable form
fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        format!("{}m {}s", mins, secs)
    } else {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        format!("{}h {}m", hours, mins)
    }
}

/// Results for a single URL benchmark
pub struct UrlBenchmarkResults {
    pub url: String,
    pub results: Vec<BenchmarkResult>,
    pub analyses: Vec<AnalysisResult>,
    pub summary: BenchmarkSummary,
}

/// Generate the full report as a plain text string (for saving to file)
pub fn generate_report_text(
    config: &BenchmarkConfig,
    url_results: &[UrlBenchmarkResults],
) -> String {
    let mut report = String::new();

    // Header
    writeln!(report, "{}", SEPARATOR).unwrap();
    writeln!(report, "ohabench - HTTP Load Testing Report").unwrap();
    writeln!(report, "{}", SEPARATOR).unwrap();
    writeln!(report).unwrap();

    // Config summary
    if config.urls.len() == 1 {
        writeln!(report, "Target:       {}", config.urls[0]).unwrap();
    } else {
        writeln!(report, "Targets:      {} URLs", config.urls.len()).unwrap();
        for (i, url) in config.urls.iter().enumerate() {
            writeln!(report, "              {}. {}", i + 1, url).unwrap();
        }
    }
    writeln!(report, "Method:       {}", config.method).unwrap();
    writeln!(report, "Mode:         {} ramping", config.ramping.mode).unwrap();
    writeln!(
        report,
        "Range:        {} -> {} req/s",
        config.ramping.start_rate, config.ramping.max_rate
    )
    .unwrap();
    writeln!(
        report,
        "Duration:     {}s per step",
        config.ramping.duration_seconds
    )
    .unwrap();
    writeln!(
        report,
        "Break when:   Error rate > {}% OR p99 > {}ms",
        config.thresholds.max_error_rate, config.thresholds.max_p99_ms
    )
    .unwrap();
    writeln!(report, "{}", SEPARATOR).unwrap();

    // Results for each URL
    for (i, url_result) in url_results.iter().enumerate() {
        writeln!(report).unwrap();
        if url_results.len() > 1 {
            writeln!(report, "{}", SEPARATOR).unwrap();
            writeln!(
                report,
                "[{}/{}] {}",
                i + 1,
                url_results.len(),
                url_result.url
            )
            .unwrap();
            writeln!(report, "{}", SEPARATOR).unwrap();
        }
        writeln!(report).unwrap();

        // Table header
        writeln!(
            report,
            "{:>7} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8} {:>9} {:>7}",
            "Target", "Actual", "Avg Lat", "p50", "p90", "p99", "Max", "Err Rate", "Status"
        )
        .unwrap();
        writeln!(
            report,
            "{:>7} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8} {:>9} {:>7}",
            "-------",
            "---------",
            "---------",
            "--------",
            "--------",
            "--------",
            "--------",
            "---------",
            "-------"
        )
        .unwrap();

        // Table rows
        for (result, analysis) in url_result.results.iter().zip(url_result.analyses.iter()) {
            let status_str = match analysis.status {
                StepStatus::Ok => "OK",
                StepStatus::Warning => "WARN",
                StepStatus::Break => "BREAK",
                StepStatus::RateLimited => "RATE",
                StepStatus::Blocked => "BLOCK",
                StepStatus::Hung => "HANG",
            };

            writeln!(
                report,
                "{:>7} {:>9.1} {:>9} {:>8} {:>8} {:>8} {:>8} {:>8.2}% {:>6}",
                result.target_rate,
                result.actual_rate,
                format_latency(result.avg_latency_ms),
                format_latency(result.p50_latency_ms),
                format_latency(result.p90_latency_ms),
                format_latency(result.p99_latency_ms),
                format_latency(result.max_latency_ms),
                result.error_rate,
                status_str
            )
            .unwrap();
        }

        // Summary for this URL
        writeln!(report).unwrap();
        writeln!(report, "RESULTS:").unwrap();

        if let Some(rate) = url_result.summary.breaking_point_rate {
            let label = if url_result.summary.was_rate_limited {
                "Rate limited at:"
            } else if url_result.summary.was_blocked {
                "Blocked at:"
            } else {
                "Breaking point:"
            };
            writeln!(
                report,
                "  {:<18} {} req/s ({})",
                label, rate, url_result.summary.break_reason
            )
            .unwrap();
        } else {
            writeln!(
                report,
                "  Breaking point:     Not reached (consider increasing max rate)"
            )
            .unwrap();
        }

        // Display aggregated HTTP error codes if any
        if !url_result.summary.aggregated_error_codes.is_empty() {
            let error_codes_str: Vec<String> = url_result
                .summary
                .aggregated_error_codes
                .iter()
                .map(|(code, count)| format!("{} ({})", code, format_number(*count)))
                .collect();
            writeln!(
                report,
                "  HTTP errors:        {}",
                error_codes_str.join(", ")
            )
            .unwrap();
        }

        if let Some(rate) = url_result.summary.last_stable_rate {
            writeln!(report, "  Last stable rate:   {} req/s", rate).unwrap();
        }

        if let Some(rate) = url_result.summary.recommended_rate {
            writeln!(
                report,
                "  Recommended rate:   {} req/s (80% of last stable)",
                rate
            )
            .unwrap();
        }

        writeln!(
            report,
            "  Total requests:     ~{}",
            format_number(url_result.summary.total_requests)
        )
        .unwrap();

        writeln!(
            report,
            "  Total duration:     {}",
            format_duration(url_result.summary.total_duration_seconds)
        )
        .unwrap();
    }

    // Legend
    writeln!(report).unwrap();
    writeln!(report, "{}", SEPARATOR).unwrap();
    writeln!(report, "LEGEND").unwrap();
    writeln!(report, "{}", SEPARATOR).unwrap();
    writeln!(report, "Target      - Requested rate in requests/second").unwrap();
    writeln!(
        report,
        "Actual      - Achieved throughput (lower than target = saturation)"
    )
    .unwrap();
    writeln!(report, "Avg Lat     - Mean response latency").unwrap();
    writeln!(
        report,
        "p50/p90/p99 - Latency percentiles (50% / 90% / 99% of requests faster than this)"
    )
    .unwrap();
    writeln!(report, "Max         - Maximum observed latency").unwrap();
    writeln!(
        report,
        "Err Rate    - Non-2xx responses + connection/timeout errors as percentage"
    )
    .unwrap();
    writeln!(report).unwrap();
    writeln!(report, "Status Codes:").unwrap();
    writeln!(
        report,
        "  OK    = under threshold      WARN  = approaching threshold"
    )
    .unwrap();
    writeln!(
        report,
        "  BREAK = server breaking      RATE  = rate limited (429)"
    )
    .unwrap();
    writeln!(
        report,
        "  BLOCK = blocked by WAF (403)   HANG  = server hung (timed out)"
    )
    .unwrap();
    writeln!(report).unwrap();
    writeln!(report, "Expected Error Rates by Service Type:").unwrap();
    writeln!(report, "  Payment/Checkout:         < 0.1%").unwrap();
    writeln!(report, "  Core App Functionality:   < 0.5%").unwrap();
    writeln!(report, "  APIs:                     < 1%").unwrap();
    writeln!(report, "  Non-critical Features:    < 2%").unwrap();
    writeln!(report).unwrap();
    writeln!(
        report,
        "Recommended rate is 80% of last stable rate for safety margin."
    )
    .unwrap();
    writeln!(report, "{}", SEPARATOR).unwrap();

    report
}

/// Save report to file
pub fn save_report(path: &str, content: &str) -> std::io::Result<()> {
    // Create parent directories if they don't exist
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut file = std::fs::File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}
