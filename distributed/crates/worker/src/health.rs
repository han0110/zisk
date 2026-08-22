//! Health surface an external supervisor probes to decide whether the worker
//! process has to be replaced.
//!
//! Liveness alone cannot answer that. The runtime is multi threaded, so a
//! handler keeps answering while the event loop is blocked, and a worker stuck
//! on the prover mutex holds its coordinator connection open, stays registered,
//! and accepts tasks it can never run. The endpoint therefore reports the two
//! conditions the process knows about itself, a prover left in a state the next
//! job cannot reuse and an event loop that has stopped turning.
//!
//! Everything lives here and the rest of the crate only calls [`serve`],
//! [`tick`], and [`mark_unrecoverable`], so the surface stays in one file.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{LazyLock, OnceLock};
use std::time::Instant;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

/// Names the loopback port the endpoint binds. Leaving it unset keeps the
/// endpoint off, so the default build behaves exactly as before.
const PORT_ENV: &str = "ZISK_WORKER_HEALTH_PORT";

/// Prefix on every body, so a probe can tell this endpoint from whatever else
/// answers on the port. The worker shares the host network namespace, where no
/// port is reserved for it and an unrelated service can already hold the one it
/// was given.
const MARKER: &str = "zisk-worker";

/// Longest a healthy iteration can legitimately take, being the two bounded
/// drains at `WorkerNodeGrpc::COMPUTE_DRAIN_TIMEOUT` plus the input and hints
/// waits in `validate_subdir`, all four of which run inline on the loop. An
/// idle loop instead wakes every heartbeat interval, far inside this.
const LONGEST_HEALTHY_ITERATION_SECONDS: i64 = 2 * 10 + 2 * 60;

/// Four times the longest healthy iteration, so only a loop blocked well past
/// every bounded wait in the worker reports stalled. Known bad states are
/// caught by [`mark_unrecoverable`] instead, which leaves this purely a
/// backstop for blocks nobody has characterised, where a false restart costs
/// far more than a late one.
const STALE_AFTER_SECONDS: i64 = LONGEST_HEALTHY_ITERATION_SECONDS * 4;

/// Seconds since [`START`] at the most recent worker loop iteration.
static LAST_TICK: AtomicI64 = AtomicI64::new(0);

/// Set once the prover reaches a state the next job cannot reuse.
static UNRECOVERABLE: AtomicBool = AtomicBool::new(false);

/// Why the process was marked unrecoverable. First writer wins, so an operator
/// reads the original cause rather than a later cascade.
static REASON: OnceLock<&'static str> = OnceLock::new();

/// Origin the tick clock counts from. An `Instant` has no numeric form and
/// cannot live in an atomic, so ticks are stored as seconds since this fixed
/// point. Monotonic on purpose, so a wall clock correction can never age a live
/// worker into looking stalled.
static START: LazyLock<Instant> = LazyLock::new(Instant::now);

fn now_seconds() -> i64 {
    START.elapsed().as_secs() as i64
}

/// Record an iteration of either worker loop, the reconnect loop or the event
/// loop nested inside it. Ticking both keeps a worker waiting on an unreachable
/// coordinator healthy, since only the outer loop turns then, while still
/// catching a blocked event loop because that blocks the outer loop with it.
pub(crate) fn tick() {
    LAST_TICK.store(now_seconds(), Ordering::Relaxed);
}

/// Marks the process unrecoverable, so a supervisor replaces it instead of the
/// worker accepting work it can only corrupt. `reason` reaches the endpoint
/// body and stays short and specific.
pub(crate) fn mark_unrecoverable(reason: &'static str) {
    let _ = REASON.set(reason);
    UNRECOVERABLE.store(true, Ordering::Release);
}

/// Health verdict paired with the one line body that explains it.
fn report() -> (bool, String) {
    if UNRECOVERABLE.load(Ordering::Acquire) {
        let reason = REASON.get().copied().unwrap_or("reason unrecorded");
        return (false, format!("{MARKER} unrecoverable, {reason}\n"));
    }

    let idle_seconds = now_seconds() - LAST_TICK.load(Ordering::Relaxed);
    if idle_seconds > STALE_AFTER_SECONDS {
        return (false, format!("{MARKER} stalled, event loop idle {idle_seconds}s\n"));
    }

    (true, format!("{MARKER} ok, event loop idle {idle_seconds}s\n"))
}

/// Answer one probe and close. The request is drained first so the client reads
/// the response instead of a connection reset.
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

/// Binds the endpoint's socket ready for the reactor to adopt.
fn bind(port: u16) -> std::io::Result<std::net::TcpListener> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", port))?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}

/// Start the endpoint when [`PORT_ENV`] names a port, otherwise do nothing.
///
/// Binding loopback keeps the port off the cluster network while staying
/// reachable from a probe running inside the worker's own container, which
/// shares the host network namespace. That same namespace lets an unrelated
/// host service hold the port first, which leaves this worker unsupervised
/// while the probe reads the other service and calls it healthy. A failed bind
/// therefore ends the process, so the deployment fails at once and visibly.
pub(crate) fn serve() {
    let Some(port) = std::env::var(PORT_ENV).ok().and_then(|port| port.parse::<u16>().ok()) else {
        return;
    };

    // Seed before the first iteration so a worker still starting up does not
    // read as stalled.
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

    /// Names the environment variable that puts the re-executed test binary on
    /// the child branch of the bind test.
    const BIND_CHILD_ENV: &str = "ZISK_WORKER_HEALTH_BIND_CHILD";

    /// A worker that cannot bind its port is invisible to its supervisor, which
    /// is the failure this whole module exists to prevent, so it has to end the
    /// process. Driven by re-executing the test binary, because the parent
    /// cannot observe an exit any other way.
    #[test]
    fn a_taken_health_port_ends_the_process() {
        if std::env::var(BIND_CHILD_ENV).is_ok() {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async { serve() });
            std::process::exit(0);
        }

        // Held for the child's whole life, so the port it is given is taken.
        let squatter = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = squatter.local_addr().unwrap().port();
        // `--exact` matches the full path, which module_path! keeps correct
        // through a rename once the crate name libtest omits is dropped. A name
        // that matches nothing runs no test and exits zero, which fails this
        // assertion rather than passing it for the wrong reason.
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

    /// Run the curl the container healthcheck runs, returning its exit code and
    /// the body with the status code appended, which is what the probe branches
    /// on. Both verdicts are an answer, so curl itself has to succeed for each.
    fn probe(address: std::net::SocketAddr) -> (Option<i32>, String) {
        let output = std::process::Command::new("curl")
            .args(["-sS", "-m", "4", "-w", "%{http_code}", &format!("http://{address}/health")])
            .output()
            .expect("curl has to be installed to verify the probe contract");
        (output.status.code(), String::from_utf8(output.stdout).unwrap())
    }

    /// Both unhealthy conditions, the recovery back to healthy, and the wire
    /// format, driven in one test because they share process-wide state that
    /// parallel tests would race. The response has to parse as HTTP or the probe
    /// fails against a healthy worker and restarts it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn report_covers_both_unhealthy_conditions_and_the_wire_format() {
        tick();
        assert!(report().0, "a fresh tick with no flag reports healthy");

        LAST_TICK.store(now_seconds() - STALE_AFTER_SECONDS - 1, Ordering::Relaxed);
        let (healthy, body) = report();
        assert!(!healthy);
        assert!(body.starts_with("zisk-worker stalled"), "got {body}");

        // The flag outranks the staleness window, so an operator sees the
        // terminal condition rather than a generic stall.
        mark_unrecoverable("test reason");
        tick();
        let (healthy, body) = report();
        assert!(!healthy);
        assert!(body.starts_with("zisk-worker unrecoverable"), "got {body}");

        UNRECOVERABLE.store(false, Ordering::Release);
        assert!(report().0, "clearing the flag after a tick reports healthy again");

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            respond(socket).await;
        });

        UNRECOVERABLE.store(true, Ordering::Release);
        let mut client = TcpStream::connect(address).await.unwrap();
        client.write_all(b"GET /health HTTP/1.1\r\n\r\n").await.unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).await.unwrap();
        UNRECOVERABLE.store(false, Ordering::Release);

        assert!(response.starts_with("HTTP/1.1 503 Service Unavailable\r\n"), "got {response}");
        assert!(
            response.ends_with("\r\n\r\nzisk-worker unrecoverable, test reason\n"),
            "got {response}"
        );

        // curl has to accept the hand written response, and the container probe
        // branches on the marker and the status code, so all three are pinned
        // rather than assumed.
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                respond(socket).await;
            }
        });

        tick();
        let (code, output) = tokio::task::spawn_blocking(move || probe(address)).await.unwrap();
        assert_eq!(code, Some(0), "curl accepts the healthy response");
        assert!(output.starts_with("zisk-worker "), "got {output}");
        assert!(output.ends_with("200"), "got {output}");

        UNRECOVERABLE.store(true, Ordering::Release);
        let (code, output) = tokio::task::spawn_blocking(move || probe(address)).await.unwrap();
        UNRECOVERABLE.store(false, Ordering::Release);
        assert_eq!(code, Some(0), "an unhealthy verdict is still an answer curl accepts");
        assert!(output.starts_with("zisk-worker "), "got {output}");
        assert!(output.ends_with("503"), "got {output}");
    }
}
