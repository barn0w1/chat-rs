use std::{
    fmt,
    future::{Future, IntoFuture},
    io,
    net::SocketAddr,
};

use tokio::net::TcpListener;
use tokio::{sync::oneshot, time};

use crate::{
    Config, app,
    auth::{OidcError, OidcProvider},
    realtime::{RealtimeHub, RealtimeSettings},
    sqlite::{OpenError, SqliteStore},
};

/// Runs the configured server until a supported shutdown signal is received.
pub async fn run(config: Config) -> Result<(), RunError> {
    run_until(config, shutdown_signal()).await
}

async fn run_until<F>(config: Config, shutdown: F) -> Result<(), RunError>
where
    F: Future<Output = ()> + Send + 'static,
{
    tracing::info!(
        listen_addr = %config.listen_addr(),
        database_path = %config.database_path().display(),
        admission_mode = ?config.admission_mode(),
        "configuration accepted"
    );

    let store = SqliteStore::open(config.database_path())
        .await
        .map_err(RunError::Database)?;
    tracing::info!(
        database_path = %config.database_path().display(),
        "SQLite opened and migrated"
    );

    let oidc = match config.oidc() {
        Some(oidc_config) => {
            match OidcProvider::discover(oidc_config.clone(), config.public_url()).await {
                Ok(provider) => {
                    tracing::info!(issuer = %oidc_config.issuer(), "OIDC provider discovered");
                    Some(provider)
                }
                Err(error) => {
                    store.close().await;
                    return Err(RunError::Oidc(OidcStartupError(error)));
                }
            }
        }
        None => None,
    };

    let listener = match TcpListener::bind(config.listen_addr()).await {
        Ok(listener) => listener,
        Err(source) => {
            store.close().await;
            return Err(RunError::Bind {
                address: config.listen_addr(),
                source,
            });
        }
    };

    let listen_addr = match listener.local_addr() {
        Ok(address) => address,
        Err(source) => {
            store.close().await;
            return Err(RunError::ListenerAddress(source));
        }
    };
    tracing::info!(%listen_addr, "listener bound");

    let realtime = RealtimeHub::new(RealtimeSettings::default());
    let realtime_settings = realtime.settings();
    let app = app::router(store.clone(), &config, oidc, realtime.clone());
    tracing::info!(%listen_addr, "server ready");

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let mut shutdown_tx = Some(shutdown_tx);
    let serve = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.await;
        })
        .into_future();
    tokio::pin!(serve);
    tokio::pin!(shutdown);

    let serve_result = tokio::select! {
        result = &mut serve => {
            realtime.shutdown();
            if let Some(sender) = shutdown_tx.take() {
                let _ = sender.send(());
            }
            match time::timeout(
                realtime_settings.server_drain_timeout,
                realtime.wait_for_drain(),
            )
            .await
            {
                Ok(()) => result.map_err(RunError::Serve),
                Err(_) => Err(RunError::ShutdownTimeout),
            }
        }
        () = &mut shutdown => {
            realtime.shutdown();
            if let Some(sender) = shutdown_tx.take() {
                let _ = sender.send(());
            }
            match time::timeout(
                realtime_settings.server_drain_timeout,
                async {
                    let result = (&mut serve).await;
                    realtime.wait_for_drain().await;
                    result
                },
            )
            .await
            {
                Ok(result) => result.map_err(RunError::Serve),
                Err(_) => Err(RunError::ShutdownTimeout),
            }
        }
    };
    tracing::info!("HTTP serving stopped");

    store.close().await;
    tracing::info!("SQLite pool closed");

    serve_result
}

async fn shutdown_signal() {
    match wait_for_shutdown_signal().await {
        Ok(signal) => tracing::info!(signal, "shutdown signal received"),
        Err(error) => {
            tracing::error!(error = %error, "shutdown signal handling failed; shutting down")
        }
    }
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() -> io::Result<&'static str> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result?;
            Ok("SIGINT")
        }
        received = terminate.recv() => {
            received
                .map(|()| "SIGTERM")
                .ok_or_else(|| io::Error::other("SIGTERM listener closed"))
        }
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal() -> io::Result<&'static str> {
    tokio::signal::ctrl_c().await?;
    Ok("Ctrl-C")
}

/// Describes why the server could not start or finish serving.
#[derive(Debug)]
pub enum RunError {
    /// SQLite could not be opened or migrated.
    Database(OpenError),
    /// OpenID Connect provider discovery or client setup failed.
    Oidc(OidcStartupError),
    /// The configured address could not be bound.
    Bind {
        /// Requested listen address.
        address: SocketAddr,
        /// Socket bind error.
        source: io::Error,
    },
    /// The bound listener's effective address could not be inspected.
    ListenerAddress(io::Error),
    /// Axum serving ended with an unexpected I/O error.
    Serve(io::Error),
    /// HTTP or WebSocket connections did not drain within the shutdown deadline.
    ShutdownTimeout,
}

/// OpenID Connect provider discovery or client setup failure.
#[derive(Debug)]
pub struct OidcStartupError(OidcError);

impl fmt::Display for OidcStartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("provider setup failed")
    }
}

impl std::error::Error for OidcStartupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl fmt::Display for RunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(_) => formatter.write_str("failed to prepare SQLite"),
            Self::Oidc(_) => formatter.write_str("failed to prepare OpenID Connect"),
            Self::Bind { address, .. } => {
                write!(formatter, "failed to bind HTTP listener at {address}")
            }
            Self::ListenerAddress(_) => {
                formatter.write_str("failed to inspect the bound HTTP listener")
            }
            Self::Serve(_) => formatter.write_str("HTTP server failed"),
            Self::ShutdownTimeout => formatter.write_str("server shutdown timed out"),
        }
    }
}

impl std::error::Error for RunError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::Oidc(error) => Some(error),
            Self::Bind { source, .. } | Self::ListenerAddress(source) | Self::Serve(source) => {
                Some(source)
            }
            Self::ShutdownTimeout => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, ffi::OsString, future};

    use tempfile::TempDir;

    use super::*;

    fn test_config(directory: &TempDir, listen_addr: SocketAddr) -> Config {
        Config::from_values(crate::config::ConfigValues {
            listen_addr: Some(OsString::from(listen_addr.to_string())),
            database_path: Some(directory.path().join("chat.sqlite3").into_os_string()),
            ..crate::config::ConfigValues::default()
        })
        .expect("test configuration is valid")
    }

    #[test]
    fn oidc_startup_error_keeps_context_separate_from_its_source() {
        let error = OidcStartupError(OidcError::Discovery(String::from("issuer mismatch")));

        assert_eq!(error.to_string(), "provider setup failed");
        assert_eq!(
            error.source().expect("source is retained").to_string(),
            "OIDC provider discovery failed: issuer mismatch"
        );
    }

    #[tokio::test]
    async fn injected_shutdown_stops_server_and_leaves_database_reopenable() {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let config = test_config(&directory, SocketAddr::from(([127, 0, 0, 1], 0)));

        run_until(config, future::ready(()))
            .await
            .expect("injected shutdown is graceful");

        let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("database can be reopened after shutdown");
        store.close().await;
    }

    #[tokio::test]
    async fn bind_failure_closes_the_prepared_database() {
        let occupied = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .expect("test listener can be bound");
        let address = occupied
            .local_addr()
            .expect("bound listener has a local address");
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let config = test_config(&directory, address);

        let error = run_until(config, future::pending())
            .await
            .expect_err("the occupied address must be rejected");
        assert!(matches!(error, RunError::Bind { address: bound, .. } if bound == address));

        drop(occupied);
        let store = SqliteStore::open(directory.path().join("chat.sqlite3"))
            .await
            .expect("migrated database remains reopenable");
        store.close().await;
    }

    #[tokio::test]
    async fn invalid_database_location_fails_before_binding() {
        let directory = tempfile::tempdir().expect("temporary directory can be created");
        let config = Config::from_values(crate::config::ConfigValues {
            listen_addr: Some(OsString::from("127.0.0.1:0")),
            database_path: Some(directory.path().as_os_str().to_owned()),
            ..crate::config::ConfigValues::default()
        })
        .expect("test configuration is valid");

        let error = run_until(config, future::pending())
            .await
            .expect_err("a directory cannot be opened as a SQLite file");
        assert!(matches!(error, RunError::Database(_)));
    }
}
