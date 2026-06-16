use serde_json::Value;
use std::time::Duration;

pub async fn fetch_bootstrap(api_url: &str) -> Result<Value, String> {
    let base = api_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("Пустой URL API".to_string());
    }

    let mut candidates = vec![base.to_string()];
    if !base.ends_with(".php") {
        candidates.push(format!("{base}.php"));
    }
    if base.ends_with("/bootstrap") {
        candidates.push(base.replace("/bootstrap", "/bootstrap.php"));
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let mut last_error = String::from("API недоступен");

    for url in candidates {
        match client.get(&url).send().await {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.map_err(|e| e.to_string())?;
                if !status.is_success() {
                    last_error = format!("HTTP {status} ({url})");
                    continue;
                }
                let data: Value = serde_json::from_str(&body).map_err(|e| format!("Некорректный JSON: {e}"))?;
                if data.get("success").and_then(|v| v.as_bool()) == Some(false) {
                    let msg = data
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Bootstrap error");
                    last_error = msg.to_string();
                    continue;
                }
                return Ok(data);
            }
            Err(err) => {
                last_error = format!("{err} ({url})");
            }
        }
    }

    Err(last_error)
}
