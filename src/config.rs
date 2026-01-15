use crate::cli::{AuthType, HttpMethod, RampingMode};

/// Complete benchmark configuration
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    pub urls: Vec<String>,
    pub method: HttpMethod,
    pub body: Option<String>,
    pub user_agent: String,
    pub auth: AuthConfig,
    pub headers: Vec<String>,
    pub ramping: RampingConfig,
    pub thresholds: ThresholdConfig,
    pub warmup_seconds: u32,
    pub cooldown_seconds: u32,
    pub report_dir: Option<String>,
    pub report_name: Option<String>,
}

/// Get the default downloads directory for the current OS
pub fn get_downloads_dir() -> std::path::PathBuf {
    // Try platform-specific locations
    if let Some(home) = std::env::var_os("HOME") {
        let home_path = std::path::PathBuf::from(home);

        // Check XDG user dirs first (Linux)
        if let Ok(xdg_download) = std::env::var("XDG_DOWNLOAD_DIR") {
            let xdg_path = std::path::PathBuf::from(xdg_download);
            if xdg_path.exists() {
                return xdg_path;
            }
        }

        // Standard Downloads folder
        let downloads = home_path.join("Downloads");
        if downloads.exists() {
            return downloads;
        }

        // Fallback to home directory
        return home_path;
    }

    // Windows fallback
    if let Some(userprofile) = std::env::var_os("USERPROFILE") {
        let user_path = std::path::PathBuf::from(userprofile);
        let downloads = user_path.join("Downloads");
        if downloads.exists() {
            return downloads;
        }
        return user_path;
    }

    // Last resort: current directory
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Find a unique base name where neither {name}.txt nor {name}_graph.png exist
/// Returns (txt_path, png_path)
pub fn get_unique_report_paths(dir: &str, name: &str) -> (String, String) {
    use std::path::Path;

    let dir_path = Path::new(dir);

    // Try base name first
    let txt_path = dir_path.join(format!("{}.txt", name));
    let png_path = dir_path.join(format!("{}_graph.png", name));

    if !txt_path.exists() && !png_path.exists() {
        return (
            txt_path.to_string_lossy().to_string(),
            png_path.to_string_lossy().to_string(),
        );
    }

    // Find the next available counter
    let mut counter = 2;
    loop {
        let txt_path = dir_path.join(format!("{}.{}.txt", name, counter));
        let png_path = dir_path.join(format!("{}.{}_graph.png", name, counter));

        if !txt_path.exists() && !png_path.exists() {
            return (
                txt_path.to_string_lossy().to_string(),
                png_path.to_string_lossy().to_string(),
            );
        }

        counter += 1;

        // Safety limit
        if counter > 9999 {
            return (
                dir_path
                    .join(format!("{}.{}.txt", name, counter))
                    .to_string_lossy()
                    .to_string(),
                dir_path
                    .join(format!("{}.{}_graph.png", name, counter))
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub auth_type: AuthType,
    pub username: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
    pub custom_header: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RampingConfig {
    pub mode: RampingMode,
    pub start_rate: u32,
    pub max_rate: u32,
    pub step: u32,
    pub duration_seconds: u32,
    pub threads: u32,
    pub connections: u32,
}

#[derive(Debug, Clone)]
pub struct ThresholdConfig {
    pub max_error_rate: f64,
    pub max_p99_ms: u32,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            urls: Vec::new(),
            method: HttpMethod::Get,
            body: None,
            user_agent: "ohabench/0.1.0".to_string(),
            auth: AuthConfig::default(),
            headers: Vec::new(),
            ramping: RampingConfig::default(),
            thresholds: ThresholdConfig::default(),
            warmup_seconds: 0,
            cooldown_seconds: 0,
            report_dir: None,
            report_name: None,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            auth_type: AuthType::None,
            username: None,
            password: None,
            token: None,
            custom_header: None,
        }
    }
}

impl Default for RampingConfig {
    fn default() -> Self {
        Self {
            mode: RampingMode::Linear,
            start_rate: 50,
            max_rate: 5000,
            step: 50,
            duration_seconds: 30,
            threads: 4,
            connections: 100,
        }
    }
}

impl Default for ThresholdConfig {
    fn default() -> Self {
        Self {
            max_error_rate: 5.0,
            max_p99_ms: 3000,
        }
    }
}

impl RampingConfig {
    /// Generate the sequence of rates to test
    pub fn generate_rates(&self) -> Vec<u32> {
        let mut rates = Vec::new();
        let mut current = self.start_rate;

        while current <= self.max_rate {
            rates.push(current);
            current = match self.mode {
                RampingMode::Linear => current + self.step,
                RampingMode::Exponential => {
                    let next = current * 2;
                    if next == current {
                        break; // Prevent infinite loop
                    }
                    next
                }
            };
        }

        rates
    }
}
