use anyhow::Result;
use console::style;
use dialoguer::{Confirm, Input, Select};
use std::path::PathBuf;

use crate::auth::{get_auth_type_names, index_to_auth_type};
use crate::cli::{HttpMethod, RampingMode};
use crate::config::{
    get_downloads_dir, AuthConfig, BenchmarkConfig, RampingConfig, ThresholdConfig,
};
use crate::output::print_header;
use crate::user_agent::{get_preset_names, USER_AGENT_PRESETS};

/// Session state that persists across benchmark runs (within the same session)
#[derive(Clone)]
pub struct SessionState {
    pub last_report_dir: Option<PathBuf>,
    // Settings to remember (excludes: URL, filename, auth, headers)
    pub method: HttpMethod,
    pub body: Option<String>,
    pub user_agent_idx: usize,
    pub custom_user_agent: Option<String>,
    pub ramping_mode: RampingMode,
    pub start_rate: u32,
    pub max_rate: u32,
    pub step: u32,
    pub duration_idx: usize,
    pub threads: u32,
    pub connections: u32,
    pub max_error_rate: f64,
    pub max_p99_ms: u32,
    pub warmup_idx: usize,
    pub cooldown_idx: usize,
    pub save_report: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            last_report_dir: None,
            method: HttpMethod::Get,
            body: None,
            user_agent_idx: 0,
            custom_user_agent: None,
            ramping_mode: RampingMode::Linear,
            start_rate: 50,
            max_rate: 5000,
            step: 50,
            duration_idx: 0,
            threads: 4,
            connections: 100,
            max_error_rate: 5.0,
            max_p99_ms: 5000,
            warmup_idx: 1,
            cooldown_idx: 0,
            save_report: true,
        }
    }
}

/// Run the interactive menu and return a complete BenchmarkConfig
/// Updates state with the selected values for session persistence
pub fn run_interactive_menu(state: &mut SessionState) -> Result<BenchmarkConfig> {
    print_header();
    println!();

    let mut config = BenchmarkConfig::default();

    // Target URLs (NOT remembered - always ask fresh)
    println!(
        "{}",
        style("Enter target URL(s) to benchmark (comma-separated for multiple).").dim()
    );
    println!();

    let urls_input: String = Input::new()
        .with_prompt(format!("{}", style("Target URL(s)").cyan()))
        .interact_text()?;

    // Parse URLs - split by comma and ensure protocol
    config.urls = urls_input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|url| ensure_protocol(&url))
        .collect();

    if config.urls.is_empty() {
        anyhow::bail!("At least one URL is required");
    }

    // Show how many URLs will be tested
    if config.urls.len() > 1 {
        println!(
            "{} {} URLs will be tested in sequence",
            style("→").cyan(),
            config.urls.len()
        );
        for (i, url) in config.urls.iter().enumerate() {
            println!("  {}. {}", i + 1, url);
        }
        println!();
    }

    // HTTP Method
    let methods = vec!["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];
    let method_default = match state.method {
        HttpMethod::Get => 0,
        HttpMethod::Post => 1,
        HttpMethod::Put => 2,
        HttpMethod::Patch => 3,
        HttpMethod::Delete => 4,
        HttpMethod::Head => 5,
    };
    let method_idx = Select::new()
        .with_prompt(format!("{}", style("HTTP Method").cyan()))
        .items(&methods)
        .default(method_default)
        .interact()?;
    config.method = match method_idx {
        0 => HttpMethod::Get,
        1 => HttpMethod::Post,
        2 => HttpMethod::Put,
        3 => HttpMethod::Patch,
        4 => HttpMethod::Delete,
        5 => HttpMethod::Head,
        _ => HttpMethod::Get,
    };
    state.method = config.method;

    // Request body for POST/PUT/PATCH
    if matches!(
        config.method,
        HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch
    ) {
        let body_default = state.body.clone().unwrap_or_default();
        let body: String = Input::new()
            .with_prompt(format!(
                "{} (leave empty for none)",
                style("Request body").cyan()
            ))
            .default(body_default)
            .allow_empty(true)
            .interact_text()?;
        if !body.is_empty() {
            config.body = Some(body.clone());
            state.body = Some(body);
        } else {
            state.body = None;
        }
    }

    // User-Agent
    let ua_names = get_preset_names();
    let ua_idx = Select::new()
        .with_prompt(format!("{}", style("User-Agent").cyan()))
        .items(&ua_names)
        .default(state.user_agent_idx)
        .interact()?;
    state.user_agent_idx = ua_idx;

    if ua_idx == ua_names.len() - 1 {
        // Custom
        let custom_default = state.custom_user_agent.clone().unwrap_or_default();
        let custom_ua: String = Input::new()
            .with_prompt(format!("{}", style("Custom User-Agent").cyan()))
            .default(custom_default)
            .interact_text()?;
        config.user_agent = custom_ua.clone();
        state.custom_user_agent = Some(custom_ua);
    } else {
        config.user_agent = USER_AGENT_PRESETS[ua_idx].value.to_string();
    }

    // Authentication (NOT remembered - always start fresh)
    let auth_types = get_auth_type_names();
    let auth_idx = Select::new()
        .with_prompt(format!("{}", style("Authentication").cyan()))
        .items(&auth_types)
        .default(0)
        .interact()?;

    config.auth.auth_type = index_to_auth_type(auth_idx);

    match config.auth.auth_type {
        crate::cli::AuthType::Basic => {
            let username: String = Input::new()
                .with_prompt(format!("{}", style("Username").cyan()))
                .interact_text()?;
            let password: String = Input::new()
                .with_prompt(format!("{}", style("Password").cyan()))
                .interact_text()?;
            config.auth.username = Some(username);
            config.auth.password = Some(password);
        }
        crate::cli::AuthType::Bearer => {
            let token: String = Input::new()
                .with_prompt(format!("{}", style("Bearer Token").cyan()))
                .interact_text()?;
            config.auth.token = Some(token);
        }
        crate::cli::AuthType::Header => {
            let header: String = Input::new()
                .with_prompt(format!(
                    "{} (e.g., X-API-Key: secret)",
                    style("Custom Header").cyan()
                ))
                .interact_text()?;
            config.auth.custom_header = Some(header);
        }
        crate::cli::AuthType::None => {}
    }

    // Additional headers (NOT remembered - always start fresh)
    println!(
        "{}",
        style("Example: X-API-Key: secret, Accept: application/json").dim()
    );
    let extra_headers: String = Input::new()
        .with_prompt(format!(
            "{} (comma-separated, leave empty to skip)",
            style("Additional headers").cyan()
        ))
        .allow_empty(true)
        .interact_text()?;
    if !extra_headers.is_empty() {
        config.headers = extra_headers
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    // Ramping mode
    let ramping_modes = vec![
        "Linear (50, 100, 150, 200...)",
        "Exponential (50, 100, 200, 400...)",
    ];
    let mode_default = match state.ramping_mode {
        RampingMode::Linear => 0,
        RampingMode::Exponential => 1,
    };
    let mode_idx = Select::new()
        .with_prompt(format!("{}", style("Ramping mode").cyan()))
        .items(&ramping_modes)
        .default(mode_default)
        .interact()?;
    config.ramping.mode = match mode_idx {
        0 => RampingMode::Linear,
        1 => RampingMode::Exponential,
        _ => RampingMode::Linear,
    };
    state.ramping_mode = config.ramping.mode;

    // Starting rate
    config.ramping.start_rate = Input::new()
        .with_prompt(format!("{}", style("Starting rate (req/s)").cyan()))
        .default(state.start_rate)
        .interact_text()?;
    state.start_rate = config.ramping.start_rate;

    // Maximum rate
    config.ramping.max_rate = Input::new()
        .with_prompt(format!("{}", style("Maximum rate (req/s)").cyan()))
        .default(state.max_rate)
        .interact_text()?;
    state.max_rate = config.ramping.max_rate;

    // Step size (only for linear)
    if config.ramping.mode == RampingMode::Linear {
        config.ramping.step = Input::new()
            .with_prompt(format!("{}", style("Step size").cyan()))
            .default(state.step)
            .interact_text()?;
        state.step = config.ramping.step;
    }

    // Duration per step
    let durations = vec!["30 seconds (Recommended)", "60 seconds", "120 seconds"];
    let dur_idx = Select::new()
        .with_prompt(format!("{}", style("Duration per step").cyan()))
        .items(&durations)
        .default(state.duration_idx)
        .interact()?;
    state.duration_idx = dur_idx;
    config.ramping.duration_seconds = match dur_idx {
        0 => 30,
        1 => 60,
        2 => 120,
        _ => 30,
    };

    // Threads
    config.ramping.threads = Input::new()
        .with_prompt(format!("{}", style("Threads").cyan()))
        .default(state.threads)
        .interact_text()?;
    state.threads = config.ramping.threads;

    // Connections
    config.ramping.connections = Input::new()
        .with_prompt(format!("{}", style("Connections").cyan()))
        .default(state.connections)
        .interact_text()?;
    state.connections = config.ramping.connections;

    // Breaking point thresholds
    println!();
    println!("{}", style("Breaking point thresholds:").yellow().bold());

    config.thresholds.max_error_rate = Input::new()
        .with_prompt(format!("{}", style("Max error rate (%)").cyan()))
        .default(state.max_error_rate)
        .interact_text()?;
    state.max_error_rate = config.thresholds.max_error_rate;

    config.thresholds.max_p99_ms = Input::new()
        .with_prompt(format!("{}", style("Max p99 latency (ms)").cyan()))
        .default(state.max_p99_ms)
        .interact_text()?;
    state.max_p99_ms = config.thresholds.max_p99_ms;

    // Warmup period
    let warmup_options = vec!["No warmup", "10 seconds (Recommended)", "30 seconds"];
    let warmup_idx = Select::new()
        .with_prompt(format!("{}", style("Warmup period before ramping").cyan()))
        .items(&warmup_options)
        .default(state.warmup_idx)
        .interact()?;
    state.warmup_idx = warmup_idx;
    config.warmup_seconds = match warmup_idx {
        0 => 0,
        1 => 10,
        2 => 30,
        _ => 10,
    };

    // Cooldown between steps
    let cooldown_options = vec![
        "No cooldown (Recommended)",
        "3 seconds",
        "5 seconds",
        "10 seconds",
    ];
    let cooldown_idx = Select::new()
        .with_prompt(format!("{}", style("Cooldown between steps").cyan()))
        .items(&cooldown_options)
        .default(state.cooldown_idx)
        .interact()?;
    state.cooldown_idx = cooldown_idx;
    config.cooldown_seconds = match cooldown_idx {
        0 => 0,
        1 => 3,
        2 => 5,
        3 => 10,
        _ => 5,
    };

    // Report file (txt)
    let save_report = Confirm::new()
        .with_prompt(format!("{}", style("Save report to file?").cyan()))
        .default(state.save_report)
        .interact()?;
    state.save_report = save_report;

    if save_report {
        // Use last report dir if available, otherwise use downloads
        let default_dir = state
            .last_report_dir
            .clone()
            .unwrap_or_else(get_downloads_dir)
            .to_string_lossy()
            .to_string();

        let folder_input: String = Input::new()
            .with_prompt(format!("{}", style("Report folder").cyan()))
            .default(default_dir)
            .interact_text()?;

        // If user provided a file path, extract directory and use filename as name hint
        let folder_path = std::path::Path::new(&folder_input);
        let (report_dir, name_hint) = if folder_path.extension().is_some() {
            // Looks like a file path - extract parent dir and stem
            let parent = folder_path
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string());
            let stem = folder_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string());
            (parent, stem)
        } else {
            (folder_input.clone(), None)
        };

        // Update last_report_dir for next run
        state.last_report_dir = Some(PathBuf::from(&report_dir));

        // Default benchmark name - use URL if single target, otherwise generic name
        // (NOT remembered - derived from URL each time)
        let default_name = name_hint.unwrap_or_else(|| {
            if config.urls.len() == 1 {
                url_to_safe_name(&config.urls[0])
            } else {
                "ohabench_benchmark".to_string()
            }
        });

        let benchmark_name: String = Input::new()
            .with_prompt(format!("{}", style("Benchmark name").cyan()))
            .default(default_name)
            .interact_text()?;

        config.report_dir = Some(report_dir);
        config.report_name = Some(benchmark_name);
    }

    Ok(config)
}

/// Build config from CLI args
pub fn config_from_args(args: &crate::cli::Args) -> BenchmarkConfig {
    use crate::user_agent::resolve_user_agent;

    BenchmarkConfig {
        urls: args.url.iter().map(|u| ensure_protocol(u)).collect(),
        method: args.method,
        body: args.body.clone(),
        user_agent: resolve_user_agent(&args.user_agent),
        auth: AuthConfig {
            auth_type: args.auth_type,
            username: args.auth_user.clone(),
            password: args.auth_pass.clone(),
            token: args.auth_token.clone(),
            custom_header: args.auth_header.clone(),
        },
        headers: args.headers.clone(),
        ramping: RampingConfig {
            mode: args.mode,
            start_rate: args.start_rate,
            max_rate: args.max_rate,
            step: args.step,
            duration_seconds: args.duration,
            threads: args.threads,
            connections: args.connections,
        },
        thresholds: ThresholdConfig {
            max_error_rate: args.max_error_rate,
            max_p99_ms: args.max_p99,
        },
        warmup_seconds: args.warmup,
        cooldown_seconds: args.cooldown,
        report_dir: args.report_dir.clone(),
        report_name: args.report_name.clone(),
    }
}

/// Ensure URL has a protocol, defaulting to https://
fn ensure_protocol(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    }
}

/// Convert URL to a safe filename (without extension)
fn url_to_safe_name(url: &str) -> String {
    let mut name = url.to_string();

    // Remove protocol
    name = name.replace("https://", "").replace("http://", "");

    // Replace unsafe characters
    name = name
        .replace('/', "_")
        .replace(':', "_")
        .replace('?', "_")
        .replace('&', "_")
        .replace('=', "_")
        .replace('#', "_")
        .replace(' ', "_");

    // Remove trailing underscores
    while name.ends_with('_') {
        name.pop();
    }

    // Limit length
    if name.len() > 100 {
        name = name[..100].to_string();
    }

    name
}
