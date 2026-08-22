use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{LazyLock, OnceLock};
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

const PORT_ENV: &str = "ZISK_WORKER_HEALTH_PORT";
const MARKER: &str = "zisk-worker";
// Two drains at COMPUTE_DRAIN_TIMEOUT plus the two waits in validate_subdir.
const LONGEST_HEALTHY_ITERATION_SECONDS: i64 = 2 * 10 + 2 * 60;
const STALE_AFTER_SECONDS: i64 = LONGEST_HEALTHY_ITERATION_SECONDS * 4;
static LAST_TICK: AtomicI64 = AtomicI64::new(0);
static REASON: OnceLock<&'static str> = OnceLock::new();
static START: LazyLock<Instant> = LazyLock::new(Instant::now);

fn now_seconds() -> i64 {
    START.elapsed().as_secs() as i64
}

pub(crate) fn tick() {
    LAST_TICK.store(now_seconds(), Ordering::Relaxed);
}

pub(crate) fn mark_unrecoverable(reason: &'static str) {
    let _ = REASON.set(reason);
}

fn report() -> (bool, String) {
    if let Some(reason) = REASON.get() {
        return (false, format!("{MARKER} unrecoverable, {reason}\n"));
    }

    let idle_seconds = now_seconds() - LAST_TICK.load(Ordering::Relaxed);
    if idle_seconds > STALE_AFTER_SECONDS {
        return (false, format!("{MARKER} stalled, event loop idle {idle_seconds}s\n"));
    }

    (true, format!("{MARKER} ok, event loop idle {idle_seconds}s\n"))
}

async fn respond(mut socket: TcpStream) {
    let mut request = [0u8; 512];
    let _ = socket.read(&mut request).await;

    let (healthy, body) = report();
    let status = if healthy { "200 OK" } else { "503 Service Unavailable" };
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: text/plain\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = socket.write_all(response.as_bytes()).await;
}

fn bind(port: u16) -> std::io::Result<std::net::TcpListener> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

pub(crate) fn serve() {
    let Some(port) = std::env::var(PORT_ENV).ok().and_then(|port| port.parse::<u16>().ok()) else {
        return;
    };

    tick();

    let listener = bind(port).unwrap_or_else(|e| {
        error!("Health endpoint cannot bind 127.0.0.1:{port}, so nothing can supervise this worker: {e}");
        std::process::exit(1);
    });
    info!("Health endpoint listening on 127.0.0.1:{}", port);

    tokio::spawn(async move {
        let listener =
            TcpListener::from_std(listener).expect("a bound non blocking socket registers");

        loop {
            match listener.accept().await {
                Ok((socket, _)) => {
                    tokio::spawn(respond(socket));
                }
                Err(e) => error!("Health endpoint accept failed: {}", e),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIND_CHILD_ENV: &str = "ZISK_WORKER_HEALTH_BIND_CHILD";

    #[test]
    fn a_taken_health_port_ends_the_process() {
        if std::env::var(BIND_CHILD_ENV).is_ok() {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async { serve() });
            std::process::exit(0);
        }

        let squatter = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = squatter.local_addr().unwrap().port();
        let module = module_path!().split_once("::").unwrap().1;
        let name = format!("{module}::a_taken_health_port_ends_the_process");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([&name, "--exact"])
            .env(BIND_CHILD_ENV, "1")
            .env(PORT_ENV, port.to_string())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(1), "a worker without its health port has to exit");
    }

    #[tokio::test]
    async fn report_covers_both_unhealthy_conditions_and_the_wire_format() {
        tick();
        assert!(report().0, "a fresh tick with no flag reports healthy");

        LAST_TICK.store(now_seconds() - STALE_AFTER_SECONDS - 1, Ordering::Relaxed);
        let (healthy, body) = report();
        assert!(!healthy);
        assert!(body.starts_with("zisk-worker stalled"), "got {body}");

        mark_unrecoverable("test reason");
        tick();
        let (healthy, body) = report();
        assert!(!healthy);
        assert!(body.starts_with("zisk-worker unrecoverable"), "got {body}");

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            respond(socket).await;
        });

        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(b"GET /health HTTP/1.1\r\n\r\n").await.unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();

        assert!(response.starts_with("HTTP/1.1 503 Service Unavailable\r\n"), "got {response}");
        assert!(
            response.ends_with("\r\n\r\nzisk-worker unrecoverable, test reason\n"),
            "got {response}"
        );
    }
}
