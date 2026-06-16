use crate::install::is_install_cancelled;
use crate::ping::ping_server;
use std::time::{Duration, Instant};
use tokio::time::sleep;

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const WAIT_TIMEOUT: Duration = Duration::from_secs(180);

pub async fn wait_for_server_online<F>(host: &str, port: u16, mut on_progress: F) -> Result<(), String>
where
    F: FnMut(&str),
{
    let started = Instant::now();
    on_progress("Запуск сервера…");

    loop {
        if is_install_cancelled() {
            return Err("Установка отменена".to_string());
        }

        if started.elapsed() >= WAIT_TIMEOUT {
            return Err(format!(
                "Сервер не ответил за {} сек. Подождите и нажмите «Играть» снова.",
                WAIT_TIMEOUT.as_secs()
            ));
        }

        if ping_server(host, port).await.online {
            on_progress("Сервер готов");
            return Ok(());
        }

        let secs = started.elapsed().as_secs();
        let message = if secs < 15 {
            "Запуск сервера…".to_string()
        } else if secs < 60 {
            format!("Сервер загружается… ({secs} с)")
        } else {
            format!("Ожидание сервера… ({secs} с)")
        };
        on_progress(&message);

        sleep(POLL_INTERVAL).await;
    }
}
