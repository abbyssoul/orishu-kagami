mod config;

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use config::RuntimeConfig;
use salvo::conn::rustls::{Keycert, RustlsConfig};
use salvo::prelude::*;
use salvo::server::ServerHandle;
use tokio::signal;
use tokio::task::JoinHandle;

// ── Handlers ────────────────────────────────────────────────────────────────

#[handler]
async fn hello() -> &'static str {
    "Hello World"
}

// ── Listen address ──────────────────────────────────────────────────────────

/// A listen address: either a TCP/IP socket or a Unix domain socket.
///
/// Accepted formats:
/// - `$XDG_RUNTIME_DIR/orishu/worker.sock` — absolute path → Unix socket
/// - `./custom.sock` — relative path → Unix socket
/// - `unix:/path/to/sock` — explicit scheme → Unix socket
/// - `0.0.0.0:8698` — IPv4 + port → TCP
/// - `[::]:8698` — IPv6 + port → TCP
#[derive(Debug, Clone, PartialEq, Eq)]
enum ListenAddress {
    Tcp(SocketAddr),
    Unix(PathBuf),
}

impl std::fmt::Display for ListenAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tcp(addr) => write!(f, "{addr}"),
            Self::Unix(path) => write!(f, "{}", path.display()),
        }
    }
}

impl std::str::FromStr for ListenAddress {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with('/') || s.starts_with("./") {
            return Ok(Self::Unix(PathBuf::from(s)));
        }
        if let Some(path) = s.strip_prefix("unix:") {
            return Ok(Self::Unix(PathBuf::from(path)));
        }
        s.parse::<SocketAddr>()
            .map(Self::Tcp)
            .map_err(|e| format!("invalid listen address {s:?}: {e}"))
    }
}

// ── CLI ─────────────────────────────────────────────────────────────────────

/// orishu-worker: the orishu cluster worker daemon.
#[derive(Debug, Parser)]
#[command(name = "orishu-worker")]
struct Cli {
    /// Path to worker config file in the shared YAML format.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Listen addresses (TCP socket or Unix socket path). Repeatable.
    /// Defaults to `$XDG_RUNTIME_DIR/orishu/worker.sock` if not specified.
    #[arg(short, long = "listen.clients")]
    listen: Vec<ListenAddress>,

    /// Path to TLS certificate PEM file (enables TLS on TCP listeners).
    #[arg(long)]
    tls_cert: Option<PathBuf>,

    /// Path to TLS private key PEM file.
    #[arg(long)]
    tls_key: Option<PathBuf>,
}

// ── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let runtime = resolve_runtime_config(&cli).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });

    let tls_config = match (&runtime.tls_cert, &runtime.tls_key) {
        (Some(cert_path), Some(key_path)) => {
            let cert = std::fs::read(cert_path).expect("failed to read TLS certificate");
            let key = std::fs::read(key_path).expect("failed to read TLS private key");
            Some(RustlsConfig::new(Keycert::new().cert(cert).key(key)))
        }
        _ => None,
    };

    let mut server_handles: Vec<ServerHandle> = Vec::new();
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();

    for addr in &runtime.listen {
        let router = Router::new().get(hello);
        match addr {
            ListenAddress::Tcp(socket_addr) => {
                println!("Listening on TCP socket: {}", socket_addr);
                if let Some(ref tls) = tls_config {
                    let acceptor = TcpListener::new(*socket_addr)
                        .rustls(tls.clone())
                        .bind()
                        .await;
                    let server = Server::new(acceptor);
                    server_handles.push(server.handle());
                    tasks.push(tokio::spawn(async move {
                        server.try_serve(router).await.expect("TLS server failed");
                    }));
                } else {
                    let acceptor = TcpListener::new(*socket_addr).bind().await;
                    let server = Server::new(acceptor);
                    server_handles.push(server.handle());
                    tasks.push(tokio::spawn(async move {
                        server.try_serve(router).await.expect("TCP server failed");
                    }));
                }
            }
            ListenAddress::Unix(path) => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                        panic!(
                            "failed to create socket directory {}: {e}",
                            parent.display()
                        )
                    });
                }
                println!("Listening on Unix socket: {}", path.display());

                let acceptor = UnixListener::new(path.clone()).bind().await;
                let server = Server::new(acceptor);
                server_handles.push(server.handle());
                tasks.push(tokio::spawn(async move {
                    server.try_serve(router).await.expect("Unix server failed");
                }));
            }
        }
    }

    tokio::spawn(listen_shutdown_signal(
        server_handles,
        Some(std::time::Duration::from_secs(30)),
    ));

    for task in tasks {
        let _ = task.await;
    }
}

// ── Shutdown ────────────────────────────────────────────────────────────────

async fn listen_shutdown_signal(
    handles: Vec<ServerHandle>,
    timeout_duration: Option<std::time::Duration>,
) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(windows)]
    let terminate = async {
        signal::windows::ctrl_c()
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => println!("ctrl_c signal received"),
        _ = terminate => println!("terminate signal received"),
    };

    for handle in handles {
        handle.stop_graceful(timeout_duration);
    }
}

fn resolve_runtime_config(cli: &Cli) -> Result<RuntimeConfig, String> {
    let mut runtime = match cli.config.as_deref() {
        Some(path) => RuntimeConfig::from_file(path)?,
        None => RuntimeConfig::default(),
    };

    runtime.apply_cli(&cli.listen, cli.tls_cert.as_deref(), cli.tls_key.as_deref());
    runtime.finalize()
}
