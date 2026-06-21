use std::{ffi::OsString, fmt};

use chat_server::MAX_ADMISSION_CODE_LIFETIME_HOURS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    Serve,
    CreateAdmissionCode { valid_for_hours: u64 },
}

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, CommandError> {
    let args = args
        .into_iter()
        .map(|value| value.into_string().map_err(|_| CommandError))
        .collect::<Result<Vec<_>, _>>()?;
    match args.as_slice() {
        [] => Ok(Command::Serve),
        [resource, action, option, hours]
            if resource == "admission-code"
                && action == "create"
                && option == "--valid-for-hours" =>
        {
            let hours = hours.parse::<u64>().map_err(|_| CommandError)?;
            if !(1..=MAX_ADMISSION_CODE_LIFETIME_HOURS).contains(&hours) {
                return Err(CommandError);
            }
            Ok(Command::CreateAdmissionCode {
                valid_for_hours: hours,
            })
        }
        _ => Err(CommandError),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommandError;

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "usage: chat-server [admission-code create --valid-for-hours <1-{MAX_ADMISSION_CODE_LIFETIME_HOURS}>]"
        )
    }
}

impl std::error::Error for CommandError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(|value| OsString::from(*value)).collect()
    }

    #[test]
    fn no_arguments_select_server_mode() {
        assert_eq!(parse(Vec::<OsString>::new()), Ok(Command::Serve));
    }

    #[test]
    fn create_command_requires_a_bounded_positive_lifetime() {
        assert_eq!(
            parse(args(&[
                "admission-code",
                "create",
                "--valid-for-hours",
                "24",
            ])),
            Ok(Command::CreateAdmissionCode {
                valid_for_hours: 24,
            })
        );
        for value in ["0", "8761", "one-day"] {
            assert!(
                parse(args(&[
                    "admission-code",
                    "create",
                    "--valid-for-hours",
                    value,
                ]))
                .is_err()
            );
        }
    }

    #[test]
    fn unknown_or_incomplete_commands_are_rejected() {
        assert!(parse(args(&["admission-code"])).is_err());
        assert!(parse(args(&["serve"])).is_err());
    }
}
