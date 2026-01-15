/// User-Agent preset definitions
#[derive(Debug, Clone)]
pub struct UserAgentPreset {
    pub name: &'static str,
    pub value: &'static str,
}

pub const USER_AGENT_PRESETS: &[UserAgentPreset] = &[
    UserAgentPreset {
        name: "ohabench (Default)",
        value: "ohabench/0.1.0",
    },
    UserAgentPreset {
        name: "Chrome (Windows)",
        value: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    },
    UserAgentPreset {
        name: "Chrome (macOS)",
        value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    },
    UserAgentPreset {
        name: "Chrome (Linux)",
        value: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    },
    UserAgentPreset {
        name: "Firefox (Windows)",
        value: "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:121.0) Gecko/20100101 Firefox/121.0",
    },
    UserAgentPreset {
        name: "Firefox (macOS)",
        value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:121.0) Gecko/20100101 Firefox/121.0",
    },
    UserAgentPreset {
        name: "Firefox (Linux)",
        value: "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0",
    },
    UserAgentPreset {
        name: "Safari (macOS)",
        value: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Safari/605.1.15",
    },
    UserAgentPreset {
        name: "Safari (iOS)",
        value: "Mozilla/5.0 (iPhone; CPU iPhone OS 17_2 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.2 Mobile/15E148 Safari/604.1",
    },
    UserAgentPreset {
        name: "Edge (Windows)",
        value: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36 Edg/120.0.0.0",
    },
    UserAgentPreset {
        name: "curl",
        value: "curl/8.5.0",
    },
    UserAgentPreset {
        name: "Googlebot",
        value: "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
    },
    UserAgentPreset {
        name: "Custom...",
        value: "",
    },
];

/// Try to resolve a user agent string from a preset name or return as custom
pub fn resolve_user_agent(input: &str) -> String {
    // Check if it matches a preset name (case-insensitive)
    let input_lower = input.to_lowercase();

    for preset in USER_AGENT_PRESETS {
        if preset.name.to_lowercase().contains(&input_lower)
            || input_lower.contains(&preset.name.to_lowercase().replace(" ", "-"))
        {
            if !preset.value.is_empty() {
                return preset.value.to_string();
            }
        }
    }

    // Check for short aliases
    match input_lower.as_str() {
        "ohabench" | "default" => USER_AGENT_PRESETS[0].value.to_string(),
        "chrome-windows" | "chrome-win" => USER_AGENT_PRESETS[1].value.to_string(),
        "chrome-macos" | "chrome-mac" => USER_AGENT_PRESETS[2].value.to_string(),
        "chrome-linux" | "chrome" => USER_AGENT_PRESETS[3].value.to_string(),
        "firefox-windows" | "firefox-win" => USER_AGENT_PRESETS[4].value.to_string(),
        "firefox-macos" | "firefox-mac" => USER_AGENT_PRESETS[5].value.to_string(),
        "firefox-linux" | "firefox" => USER_AGENT_PRESETS[6].value.to_string(),
        "safari-macos" | "safari-mac" | "safari" => USER_AGENT_PRESETS[7].value.to_string(),
        "safari-ios" | "safari-iphone" => USER_AGENT_PRESETS[8].value.to_string(),
        "edge" | "edge-windows" => USER_AGENT_PRESETS[9].value.to_string(),
        "curl" => USER_AGENT_PRESETS[10].value.to_string(),
        "googlebot" | "google" => USER_AGENT_PRESETS[11].value.to_string(),
        _ => input.to_string(), // Return as custom user agent
    }
}

/// Get preset names for menu display
pub fn get_preset_names() -> Vec<&'static str> {
    USER_AGENT_PRESETS.iter().map(|p| p.name).collect()
}
