use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{lookup_host, TcpStream};
use tokio::time::timeout;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const PACKET_TIMEOUT: Duration = Duration::from_secs(2);
const DNS_TIMEOUT: Duration = Duration::from_millis(1200);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub online: bool,
    pub players_online: u32,
    pub players_max: u32,
    pub latency_ms: u32,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    players: Option<StatusPlayers>,
}

#[derive(Debug, Deserialize)]
struct StatusPlayers {
    online: i32,
    max: i32,
}

pub async fn ping_server(host: &str, port: u16) -> PingResult {
    let offline = PingResult {
        online: false,
        players_online: 0,
        players_max: 0,
        latency_ms: 0,
    };

    let addr = match resolve(host, port).await {
        Some(a) => a,
        None => return offline,
    };

    let start = Instant::now();
    let mut stream = match timeout(CONNECT_TIMEOUT, TcpStream::connect(addr)).await {
        Ok(Ok(s)) => s,
        _ => return offline,
    };

    let host = truncate_host(host);
    let mut handshake = Vec::new();
    write_var_int(&mut handshake, 0x00);
    write_var_int(&mut handshake, 47);
    write_string(&mut handshake, host);
    handshake.extend_from_slice(&port.to_be_bytes());
    write_var_int(&mut handshake, 1);
    if timeout(PACKET_TIMEOUT, write_packet(&mut stream, &handshake))
        .await
        .is_err()
    {
        return offline;
    }

    let mut request = Vec::new();
    write_var_int(&mut request, 0x00);
    if timeout(PACKET_TIMEOUT, write_packet(&mut stream, &request))
        .await
        .is_err()
    {
        return offline;
    }

    let payload = match timeout(PACKET_TIMEOUT, read_packet(&mut stream)).await {
        Ok(Ok(p)) => p,
        _ => return offline,
    };

    let latency_ms = start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
    let mut result = PingResult {
        online: true,
        players_online: 0,
        players_max: 0,
        latency_ms,
    };

    if payload.first() == Some(&0x00) {
        let json_start = payload.iter().position(|&b| b == b'{').unwrap_or(1);
        if let Ok(status) = serde_json::from_slice::<StatusResponse>(&payload[json_start..]) {
            if let Some(players) = status.players {
                result.players_online = players.online.max(0) as u32;
                result.players_max = players.max.max(0) as u32;
            }
        }
    }

    result
}

async fn resolve(host: &str, port: u16) -> Option<SocketAddr> {
    let lookup = lookup_host((host, port));
    let mut addrs = timeout(DNS_TIMEOUT, lookup).await.ok()?.ok()?;
    addrs.next()
}

fn truncate_host(host: &str) -> &str {
    if host.len() > 255 {
        &host[..255]
    } else {
        host
    }
}

async fn write_packet(stream: &mut TcpStream, payload: &[u8]) -> Result<(), ()> {
    let mut packet = Vec::new();
    write_var_int(&mut packet, payload.len() as i32);
    packet.extend_from_slice(payload);
    stream.write_all(&packet).await.map_err(|_| ())
}

async fn read_packet(stream: &mut TcpStream) -> Result<Vec<u8>, ()> {
    let len = read_var_int(stream).await?;
    if len <= 0 || len > 65536 {
        return Err(());
    }
    let mut buf = vec![0u8; len as usize];
    stream.read_exact(&mut buf).await.map_err(|_| ())?;
    Ok(buf)
}

async fn read_var_int(stream: &mut TcpStream) -> Result<i32, ()> {
    let mut num_read = 0;
    let mut result = 0i32;
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await.map_err(|_| ())?;
        let value = byte[0];
        result |= ((value & 0x7F) as i32) << num_read;
        num_read += 7;
        if num_read > 35 {
            return Err(());
        }
        if value & 0x80 == 0 {
            break;
        }
    }
    Ok(result)
}

fn write_var_int(buf: &mut Vec<u8>, mut value: i32) {
    loop {
        let mut temp = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            temp |= 0x80;
        }
        buf.push(temp);
        if value == 0 {
            break;
        }
    }
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    write_var_int(buf, bytes.len() as i32);
    buf.extend_from_slice(bytes);
}
