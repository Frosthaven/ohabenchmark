mod analysis;
mod auth;
mod cli;
mod config;
mod graph;
mod menu;
mod output;
mod runner;
mod user_agent;

use anyhow::{bail, Result};
use clap::Parser;
use console::style;
use dialoguer::Select;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use analysis::{analyze_result, generate_summary, AnalysisResult, StepStatus};
use cli::Args;
use config::{get_unique_report_paths, BenchmarkConfig};
use menu::{config_from_args, run_interactive_menu, SessionState};
use output::{
    generate_report_text, print_config_summary, print_header, print_legend, print_result_row,
    print_summary, print_table_header, print_url_header, save_report, UrlBenchmarkResults,
};
use runner::{check_oha_installed, run_benchmark, run_warmup, BenchmarkResult};

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {:#}", style("Error:").red().bold(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // Check if oha is installed
    check_oha_installed()?;

    // Parse CLI args
    let args = Args::parse();

    // Non-interactive mode: run once and exit
    if args.non_interactive {
        if args.url.is_empty() {
            bail!("--url is required when using --non-interactive");
        }
        let config = config_from_args(&args);
        let mut state = SessionState::default();
        run_benchmark_suite(&config, &mut state)?;
        return Ok(());
    }

    // CLI mode with URL provided: run once and exit
    if !args.url.is_empty() {
        let config = config_from_args(&args);
        let mut state = SessionState::default();
        run_benchmark_suite(&config, &mut state)?;
        return Ok(());
    }

    // Interactive mode: loop until user quits
    let mut state = SessionState::default();

    loop {
        // Run interactive menu
        let config = match run_interactive_menu(&mut state) {
            Ok(c) => c,
            Err(e) => {
                // User may have pressed Ctrl+C
                eprintln!("\n{} {}", style("Cancelled:").yellow(), e);
                break;
            }
        };

        // Validate config
        if config.urls.is_empty() {
            eprintln!(
                "{} At least one target URL is required",
                style("Error:").red()
            );
            continue;
        }

        // Run the benchmark
        if let Err(e) = run_benchmark_suite(&config, &mut state) {
            eprintln!("{} {:#}", style("Error:").red().bold(), e);
        }

        // Ask what to do next
        println!();
        let choices = vec!["Run another benchmark", "Quit"];

        let selection = Select::new()
            .with_prompt(format!("{}", style("What would you like to do?").cyan()))
            .items(&choices)
            .default(0)
            .interact();

        match selection {
            Ok(0) => {
                println!();
                continue; // Run another benchmark
            }
            Ok(1) | Err(_) => {
                println!();
                println!("{}", style("Goodbye!").green());
                break;
            }
            _ => break,
        }
    }

    Ok(())
}

fn run_benchmark_suite(config: &BenchmarkConfig, state: &mut SessionState) -> Result<()> {
    // Print header and config summary
    print_header();
    print_config_summary(config);

    // Print legend before starting
    print_legend();

    // Generate rate sequence
    let rates = config.ramping.generate_rates();

    if rates.is_empty() {
        bail!("No rates to test. Check your start/max rate configuration.");
    }

    // Store results for all URLs
    let mut all_url_results: Vec<UrlBenchmarkResults> = Vec::new();

    // Run benchmarks for each URL
    for (url_idx, url) in config.urls.iter().enumerate() {
        // Print URL header for multi-URL runs
        if config.urls.len() > 1 {
            print_url_header(url, url_idx, config.urls.len());
        }

        // Run warmup if configured
        if config.warmup_seconds > 0 {
            println!();
            let spinner = create_spinner(&format!(
                "Warming up {} for {}s at {} req/s...",
                url, config.warmup_seconds, config.ramping.start_rate
            ));
            if let Err(e) = run_warmup(config, url) {
                spinner.finish_and_clear();
                eprintln!("{} Warmup failed for {}: {}", style("✗").red(), url, e);
                continue;
            }
            spinner.finish_and_clear();
            println!("{} Warmup complete", style("✓").green());
        }

        println!();
        println!(
            "Starting benchmark: {} steps from {} to {} req/s",
            rates.len(),
            rates.first().unwrap(),
            rates.last().unwrap()
        );

        // Print table header
        print_table_header();

        // Run benchmarks for this URL
        let mut results: Vec<BenchmarkResult> = Vec::new();
        let mut analyses: Vec<AnalysisResult> = Vec::new();

        for (i, &rate) in rates.iter().enumerate() {
            // Create progress indicator for this step
            let pb =
                create_step_progress(i + 1, rates.len(), rate, config.ramping.duration_seconds);

            // Run benchmark
            let result = match run_benchmark(config, url, rate) {
                Ok(r) => r,
                Err(e) => {
                    pb.finish_and_clear();
                    eprintln!("{} Failed at {} req/s: {}", style("✗").red(), rate, e);
                    break;
                }
            };

            pb.finish_and_clear();

            // Analyze result
            let analysis = analyze_result(&result, &config.thresholds);

            // Print row
            print_result_row(&result, &analysis);

            // Store results
            let should_break = matches!(
                analysis.status,
                StepStatus::Break
                    | StepStatus::RateLimited
                    | StepStatus::Blocked
                    | StepStatus::Hung
            );
            results.push(result);
            analyses.push(analysis);

            // Check if we should stop
            if should_break {
                break;
            }

            // Cooldown between steps (skip after last step)
            if config.cooldown_seconds > 0 && i < rates.len() - 1 {
                let cooldown_pb = create_cooldown_progress(config.cooldown_seconds);
                std::thread::sleep(Duration::from_secs(config.cooldown_seconds as u64));
                cooldown_pb.finish_and_clear();
            }
        }

        // Generate summary for this URL
        let summary = generate_summary(&results, &analyses, config.ramping.duration_seconds);

        // Print summary
        print_summary(&summary);

        // Store results for report
        all_url_results.push(UrlBenchmarkResults {
            url: url.clone(),
            results,
            analyses,
            summary,
        });
    }

    // Save report if configured
    if let (Some(ref dir), Some(ref name)) = (&config.report_dir, &config.report_name) {
        let (txt_path, png_path) = get_unique_report_paths(dir, name);

        let report = generate_report_text(config, &all_url_results);

        match save_report(&txt_path, &report) {
            Ok(_) => {
                println!();
                println!("{} Report saved to: {}", style("✓").green(), txt_path);

                // Update session state with the directory we saved to
                if let Some(parent) = std::path::Path::new(&txt_path).parent() {
                    state.last_report_dir = Some(parent.to_path_buf());
                }
            }
            Err(e) => {
                eprintln!("{} Failed to save report: {}", style("✗").red(), e);
            }
        }

        // Generate PNG graph
        match graph::generate_error_rate_graph(&all_url_results, &png_path) {
            Ok(_) => {
                println!("{} Graph saved to: {}", style("✓").green(), png_path);
            }
            Err(e) => {
                eprintln!("{} Failed to save graph: {}", style("✗").red(), e);
            }
        }
    }

    Ok(())
}

fn create_spinner(message: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

fn create_step_progress(_step: usize, _total: usize, rate: u32, duration: u32) -> ProgressBar {
    let pb = ProgressBar::new(duration as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  [{bar:20.cyan/dim}] {pos}s/{len}s @ {msg} req/s")
            .unwrap()
            .progress_chars("━━─"),
    );
    pb.set_message(rate.to_string());

    // Tick every second
    let pb_clone = pb.clone();
    std::thread::spawn(move || {
        for i in 0..=duration {
            std::thread::sleep(Duration::from_secs(1));
            pb_clone.set_position(i as u64);
        }
    });

    pb
}

fn create_cooldown_progress(duration: u32) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("  {spinner:.dim} Cooling down ({msg}s)...")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
    );
    pb.set_message(duration.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}
