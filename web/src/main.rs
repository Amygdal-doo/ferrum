use std::io::IsTerminal;

use tracing_subscriber::EnvFilter;

use web_ferrum::Config;
use web_ferrum::releases::Releases;
use web_ferrum::routes::{AppState, router};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Plain-text logs; `RUST_LOG` overrides the default `info` level.
    // ANSI colours only on a TTY so Railway's log viewer stays clean.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_ansi(std::io::stdout().is_terminal())
        .with_target(false)
        .init();

    let config = Config::from_env();
    let listener = tokio::net::TcpListener::bind(config.bind_addr()).await?;
    tracing::info!(addr = %listener.local_addr()?, "web-ferrum listening");

    let state = AppState::new(config, Releases::github());
    // Usage-count tables; in the background so the health check never
    // waits on the database. Pings fail with 503 until this has run.
    if state.usage.enabled() {
        let usage = state.usage.clone();
        tokio::spawn(async move {
            match usage.migrate().await {
                Ok(()) => tracing::info!("usage database ready"),
                Err(err) => tracing::error!(%err, "usage database migration failed"),
            }
        });
    } else {
        tracing::info!("DATABASE_URL unset: usage pings are accepted but not stored");
    }
    let service = router(state).into_make_service();
    axum::serve(listener, service)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("web-ferrum stopped");
    Ok(())
}

/// Resolves on Ctrl+C or SIGTERM (Railway sends SIGTERM on redeploy).
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(%err, "failed to listen for Ctrl+C");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => tracing::error!(%err, "failed to listen for SIGTERM"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received, draining connections");
}
