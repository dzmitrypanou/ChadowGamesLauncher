use serde_json::Value;
use std::time::Duration;

pub async fn wake_game_servers(
    api_url: &str,
    game_id: &str,
    server_id: Option<&str>,
) -> Result<(), String> {
    let base = api_base_url(api_url)?;
    let mut candidates = vec![
        format!("{base}/server-wake.php"),
        format!("{base}/server-wake"),
    ];
    candidates.dedup();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let mut last_error = String::from("API пробуждения недоступен");

    for wake_url in candidates {
        let mut url = reqwest::Url::parse(&wake_url).map_err(|e| e.to_string())?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("gameId", game_id);
            if let Some(id) = server_id.map(str::trim).filter(|value| !value.is_empty()) {
                pairs.append_pair("serverId", id);
            }
        }

        match client.get(url.clone()).send().await {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if !status.is_success() {
                    last_error = format!("HTTP {status} ({wake_url})");
                    continue;
                }

                let data: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                if data.get("success").and_then(|value| value.as_bool()) == Some(false) {
                    last_error = data
                        .get("error")
                        .and_then(|value| value.as_str())
                        .unwrap_or("wake error")
                        .to_string();
                    continue;
                }

                return Ok(());
            }
            Err(err) => {
                last_error = format!("{err} ({wake_url})");
            }
        }
    }

    Err(last_error)
}

fn api_base_url(api_url: &str) -> Result<String, String> {
    let mut base = api_url.trim().trim_end_matches('/').to_string();
    if base.is_empty() {
        return Err("Пустой URL API".to_string());
    }
    if base.ends_with(".php") {
        base = base
            .rsplit_once('/')
            .map(|(parent, _)| parent.to_string())
            .unwrap_or(base);
    }
    if base.ends_with("/bootstrap") {
        base = base
            .strip_suffix("/bootstrap")
            .unwrap_or(&base)
            .to_string();
    }
    Ok(base)
}
