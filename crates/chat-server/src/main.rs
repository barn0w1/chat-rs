mod command;
mod telemetry;

use std::{env, error::Error, fmt, process::ExitCode};

use chat_server::Config;
use command::Command;

#[tokio::main]
async fn main() -> ExitCode {
    let command = match command::parse(env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("chat-server: {error}");
            return ExitCode::FAILURE;
        }
    };

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

    match command {
        Command::Serve => match chat_server::run(config).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                tracing::error!(error = %ErrorReport(&error), "server failed");
                ExitCode::FAILURE
            }
        },
        Command::CreateAdmissionCode { valid_for_hours } => {
            match chat_server::create_admission_code(config.database_path(), valid_for_hours).await
            {
                Ok(created) => {
                    println!("admission_code={}", created.token());
                    println!("expires_at_ms={}", created.expires_at_ms());
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    tracing::error!(
                        error = %ErrorReport(&error),
                        "admission code creation failed"
                    );
                    ExitCode::FAILURE
                }
            }
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
