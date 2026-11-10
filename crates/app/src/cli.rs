//! The command line (SPEC.md, Command line). Parsing is pure and tested.

use std::path::PathBuf;

/// What the exe was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// `ramlord` and `--minimized`: run the app (M2).
    Run,
    /// `--demo`: run the app on the sample building.
    Demo,
    /// `--building FILE` with `--render OUT.png`, `--card OUT.png` or both.
    Building {
        file: PathBuf,
        render: Option<PathBuf>,
        card: Option<PathBuf>,
    },
    /// A mode SPEC.md names that a later milestone builds:
    /// `--snapshot`, `--dump` and `--self-test`.
    Later(&'static str),
}

/// Why the command line was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum UsageError {
    Unknown(String),
    MissingValue(&'static str),
    Repeated(&'static str),
    /// `--render` or `--card` without `--building`, or the reverse.
    Incomplete,
    /// Options from different modes in one command line.
    Conflict,
}

impl std::fmt::Display for UsageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsageError::Unknown(arg) => write!(f, "unknown argument {arg:?}"),
            UsageError::MissingValue(flag) => write!(f, "{flag} needs a file name"),
            UsageError::Repeated(flag) => write!(f, "{flag} is given twice"),
            UsageError::Incomplete => {
                write!(
                    f,
                    "use --building FILE with --render OUT.png, --card OUT.png or both"
                )
            }
            UsageError::Conflict => write!(f, "these options belong to different modes"),
        }
    }
}

pub const USAGE: &str = "\
usage:
  ramlord                                   run the app (Windows)
  ramlord --minimized                       start hidden in the tray (Windows)
  ramlord --demo                            run the app on the sample building (Windows)
  ramlord --building FILE --render OUT.png  draw one tower frame from a Building file
  ramlord --building FILE --card OUT.png    draw the rent roll card";

fn later(flag: &'static str, value: Option<String>) -> Result<Command, UsageError> {
    value.ok_or(UsageError::MissingValue(flag))?;
    Ok(Command::Later(flag))
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, UsageError> {
    let mut args = args.into_iter();
    let mut building = None;
    let mut render = None;
    let mut card = None;
    let mut mode: Option<Command> = None;

    fn set(
        slot: &mut Option<PathBuf>,
        flag: &'static str,
        value: Option<String>,
    ) -> Result<(), UsageError> {
        let value = value.ok_or(UsageError::MissingValue(flag))?;
        if slot.replace(PathBuf::from(value)).is_some() {
            return Err(UsageError::Repeated(flag));
        }
        Ok(())
    }

    while let Some(arg) = args.next() {
        let simple = match arg.as_str() {
            "--building" => {
                set(&mut building, "--building", args.next())?;
                continue;
            }
            "--render" => {
                set(&mut render, "--render", args.next())?;
                continue;
            }
            "--card" => {
                set(&mut card, "--card", args.next())?;
                continue;
            }
            "--minimized" => Command::Run,
            "--demo" => Command::Demo,
            // Each takes a file name; the modes themselves come in M2.
            "--snapshot" => later("--snapshot", args.next())?,
            "--dump" => later("--dump", args.next())?,
            "--self-test" => later("--self-test", args.next())?,
            _ => return Err(UsageError::Unknown(arg)),
        };
        if mode.replace(simple).is_some() {
            return Err(UsageError::Conflict);
        }
    }

    match (mode, building) {
        // `--snapshot FILE --render OUT.png` is an M2 mode.
        (Some(Command::Later(flag)), None) => Ok(Command::Later(flag)),
        (Some(_), Some(_)) => Err(UsageError::Conflict),
        (Some(_), None) if render.is_some() || card.is_some() => Err(UsageError::Conflict),
        (Some(command), None) => Ok(command),
        (None, Some(file)) if render.is_some() || card.is_some() => {
            Ok(Command::Building { file, render, card })
        }
        (None, Some(_)) => Err(UsageError::Incomplete),
        (None, None) if render.is_some() || card.is_some() => Err(UsageError::Incomplete),
        (None, None) => Ok(Command::Run),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(line: &str) -> Result<Command, UsageError> {
        parse(line.split_whitespace().map(String::from))
    }

    #[test]
    fn no_arguments_runs_the_app() {
        assert_eq!(parse_str(""), Ok(Command::Run));
        assert_eq!(parse_str("--minimized"), Ok(Command::Run));
    }

    #[test]
    fn demo() {
        assert_eq!(parse_str("--demo"), Ok(Command::Demo));
    }

    #[test]
    fn building_with_render_card_or_both() {
        assert_eq!(
            parse_str("--building b.json --render t.png"),
            Ok(Command::Building {
                file: "b.json".into(),
                render: Some("t.png".into()),
                card: None
            })
        );
        assert_eq!(
            parse_str("--card c.png --building b.json"),
            Ok(Command::Building {
                file: "b.json".into(),
                render: None,
                card: Some("c.png".into())
            })
        );
        assert_eq!(
            parse_str("--building b.json --render t.png --card c.png"),
            Ok(Command::Building {
                file: "b.json".into(),
                render: Some("t.png".into()),
                card: Some("c.png".into())
            })
        );
    }

    #[test]
    fn later_modes_are_recognised() {
        assert_eq!(parse_str("--dump out.json"), Ok(Command::Later("--dump")));
        assert_eq!(
            parse_str("--self-test out.json"),
            Ok(Command::Later("--self-test"))
        );
        assert_eq!(
            parse_str("--snapshot s.json --render t.png"),
            Ok(Command::Later("--snapshot"))
        );
    }

    #[test]
    fn usage_errors() {
        assert_eq!(parse_str("--building b.json"), Err(UsageError::Incomplete));
        assert_eq!(parse_str("--render t.png"), Err(UsageError::Incomplete));
        assert_eq!(
            parse_str("--building"),
            Err(UsageError::MissingValue("--building"))
        );
        assert_eq!(
            parse_str("--building a --building b --render t.png"),
            Err(UsageError::Repeated("--building"))
        );
        assert_eq!(
            parse_str("--demo --building b.json --render t.png"),
            Err(UsageError::Conflict)
        );
        assert_eq!(parse_str("--demo --minimized"), Err(UsageError::Conflict));
        assert_eq!(parse_str("--demo --card c.png"), Err(UsageError::Conflict));
        assert_eq!(
            parse_str("--frobnicate"),
            Err(UsageError::Unknown("--frobnicate".into()))
        );
    }
}
