//! Experimental subscription-only image provider. No HTTP client, API keys, or login flow.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::CodexProvider;

pub const MAX_PROMPT_BYTES: usize = 8_000;
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;
pub const SUPPORTED_CODEX_VERSION: &str = "0.162.0";

/// Only public account metadata is retained; never email, tokens, cookies, or session files.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub ready: bool,
    pub plan: Option<String>,
    pub message: String,
}

pub type Result<T> = std::result::Result<T, String>;

/// Cancellation is supplied by the host's background-job system. A mock implements this seam
/// in tests; the product never silently substitutes a mock image for a real generation.
pub trait Provider: Send + Sync {
    fn status(&self, cancelled: &dyn Fn() -> bool) -> Result<Status>;
    fn generate(&self, prompt: &str, cancelled: &dyn Fn() -> bool) -> Result<Vec<u8>>;
}

pub fn validate_prompt(prompt: &str) -> Result<()> {
    if prompt.trim().is_empty() || prompt.len() > MAX_PROMPT_BYTES {
        return Err(format!("Describe an image using 1 to {MAX_PROMPT_BYTES} UTF-8 bytes"));
    }
    Ok(())
}

/// Refuse API-key accounts and exhausted/unknown included limits before starting a model turn.
/// Available or unknown extra credits also block generation: native Codex may draw from them
/// automatically after a turn exhausts the included allowance.
pub fn subscription_status(account: &Value, limits: &Value) -> Status {
    let account = account.get("account").unwrap_or(&Value::Null);
    let plan = account.get("planType").and_then(Value::as_str).map(str::to_owned);
    let blocked = |message: &str| Status { ready: false, plan: plan.clone(), message: message.into() };
    if account.get("type").and_then(Value::as_str) != Some("chatgpt") {
        return blocked("Sign in to the official Codex CLI with ChatGPT first. API-key accounts are disabled.");
    }
    let buckets: Vec<&Value> = match limits.get("rateLimitsByLimitId").and_then(Value::as_object).filter(|m| !m.is_empty()) {
        Some(map) => map.values().collect(),
        None => limits.get("rateLimits").into_iter().collect(),
    };
    let mut found = false;
    for bucket in &buckets {
        for key in ["primary", "secondary"] {
            if let Some(window) = bucket.get(key).filter(|v| !v.is_null()) {
                let Some(used) = window.get("usedPercent").and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.0) else {
                    return blocked("Cannot confirm included subscription allowance. Generation is disabled.");
                };
                found = true;
                if used >= 100.0 {
                    return blocked("Included subscription allowance is exhausted. Try after the usage reset. API and extra credits are disabled.");
                }
            }
        }
    }
    if !found {
        return blocked("Cannot confirm included subscription allowance. Generation is disabled.");
    }
    let mut credits_known = false;
    for bucket in buckets {
        if let Some(credits) = bucket.get("credits").filter(|v| !v.is_null()) {
            credits_known = true;
            let balance_is_zero =
                credits.get("balance").is_none_or(|v| v.is_null() || v.as_str().and_then(|s| s.parse::<f64>().ok()).is_some_and(|n| n == 0.0));
            if credits.get("hasCredits").and_then(Value::as_bool) != Some(false)
                || credits.get("unlimited").and_then(Value::as_bool) != Some(false)
                || !balance_is_zero
            {
                return blocked("Extra credits are available or unknown. Subscription-only generation is disabled to avoid credit usage.");
            }
        }
    }
    if !credits_known {
        return blocked("Cannot confirm that extra credits are unavailable. Subscription-only generation is disabled.");
    }
    Status { ready: true, plan, message: "Connected through the official Codex CLI. Uses included subscription allowance.".into() }
}

/// Bound imported image size independently of the protocol and decoder. PNG's dimensions are
/// inspected before the host allocates a raster; the normal codec still validates the whole file.
pub fn validate_png(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_IMAGE_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || bytes.get(12..16) != Some(b"IHDR".as_slice()) {
        return Err("Codex did not return a supported bounded PNG image".into());
    }
    let number = |range: std::ops::Range<usize>| -> Option<u32> { Some(u32::from_be_bytes(bytes.get(range)?.try_into().ok()?)) };
    let (width, height) = (number(16..20), number(20..24));
    if !matches!((width, height), (Some(w), Some(h)) if w > 0 && h > 0 && w <= 4096 && h <= 4096 && w.checked_mul(h).is_some_and(|n| n <= 16_777_216)) {
        return Err("Generated image dimensions are outside the supported 4096-pixel bounds".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn api_keys_missing_limits_and_paid_credits_do_not_enable_generation() {
        let limits = json!({"rateLimits":{"primary":{"usedPercent":10},"credits":{"hasCredits":false,"unlimited":false,"balance":"0"}}});
        assert!(!subscription_status(&json!({"account":{"type":"apiKey"}}), &limits).ready);
        let account = json!({"account":{"type":"chatgpt","planType":"pro","email":"private@example.test"}});
        assert!(!subscription_status(&account, &Value::Null).ready);
        let exhausted = json!({"rateLimits":{"primary":{"usedPercent":100},"credits":{"hasCredits":true,"unlimited":false,"balance":"100"}}});
        let status = subscription_status(&account, &exhausted);
        assert!(!status.ready);
        assert!(status.message.contains("exhausted"));
        assert!(!serde_json::to_string(&status).unwrap().contains("private@example.test"));
        assert!(subscription_status(&account, &limits).ready);
        for credits in [
            Value::Null,
            json!({"hasCredits":true,"unlimited":false,"balance":"1"}),
            json!({"hasCredits":false,"unlimited":true}),
            json!({"hasCredits":false,"unlimited":false,"balance":"1"}),
        ] {
            assert!(!subscription_status(&account, &json!({"rateLimits":{"primary":{"usedPercent":10},"credits":credits}})).ready);
        }
    }

    #[test]
    fn every_reported_window_must_have_included_allowance() {
        let account = json!({"account":{"type":"chatgpt","planType":"pro"}});
        for used in [json!(100), json!(null), json!("10"), json!(-1)] {
            let limits = json!({"rateLimitsByLimitId":{"core":{"primary":{"usedPercent":0}},"images":{"secondary":{"usedPercent":used}}}});
            assert!(!subscription_status(&account, &limits).ready);
        }
    }

    #[test]
    fn prompts_and_png_headers_are_bounded() {
        for prompt in [String::new(), "  \n".into(), "a".repeat(MAX_PROMPT_BYTES + 1)] {
            assert!(validate_prompt(&prompt).is_err());
        }
        assert!(validate_prompt("A ceramic mug").is_ok());
        for bytes in [vec![], b"not an image".to_vec(), b"\x89PNG\r\n\x1a\n".to_vec()] {
            assert!(validate_png(&bytes).is_err());
        }
        let mut header = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        header.extend_from_slice(&1_u32.to_be_bytes());
        header.extend_from_slice(&1_u32.to_be_bytes());
        assert!(validate_png(&header).is_ok());
        header[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(validate_png(&header).is_err());
    }
}
