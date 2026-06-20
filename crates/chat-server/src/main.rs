mod telemetry;

use std::{error::Error, fmt, process::ExitCode};

use chat_server::Config;

#[tokio::main]
async fn main() -> ExitCode {
    if let Err(error) = telemetry::init() {
        eprintln!("chat-server: {}", ErrorReport(&error));
        return ExitCode::FAILURE;
    }

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            tracing::error!(error = %ErrorReport(&error), "configuration failed");
            return ExitCode::FAILURE;
        }
    };

    match chat_server::run(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(error = %ErrorReport(&error), "server failed");
            ExitCode::FAILURE
        }
    }
}

struct ErrorReport<'a>(&'a (dyn Error + 'static));

impl fmt::Display for ErrorReport<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(error) = source {
            write!(formatter, ": {error}")?;
            source = error.source();
        }
        Ok(())
    }
}
