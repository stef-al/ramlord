//! The command line (SPEC.md, Command line). Parsing is pure and tested.

use std::path::PathBuf;

/// What the exe was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// `ramlord`: run the app. `--minimized` starts it minimized (the tray
    /// comes with M5).
    Run { minimized: bool },
    /// `--demo`: run the app on the sample building.
    Demo,
    /// `--building FILE` with `--render OUT.png`, `--card OUT.png` or both.
    Building {
        file: PathBuf,
        render: Option<PathBuf>,
        card: Option<PathBuf>,
    },
    /// `--snapshot FILE` with `--render OUT.png`, `--card OUT.png` or both:
    /// group a Snapshot with the rules, then draw.
    Snapshot {
        file: PathBuf,
        render: Option<PathBuf>,
        card: Option<PathBuf>,
    },
    /// `--dump OUT.json`: write one live Snapshot.
    Dump(PathBuf),
    /// `--self-test OUT.json`: sample and draw off screen, check budgets.
    SelfTest(PathBuf),
}

/// Why the command line was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum UsageError {
    Unknown(String),
    MissingValue(&'static str),
    Repeated(&'static str),
    /// `--render` or `--card` without an input file, or the reverse.
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
            UsageError::Incomplete => write!(
                f,
                "use --building FILE or --snapshot FILE with --render OUT.png, --card OUT.png or both"
            ),
            UsageError::Conflict => write!(f, "these options belong to different modes"),
        }
    }
}

pub const USAGE: &str = "\
usage:
  ramlord                                   run the app (Windows)
  ramlord --minimized                       start minimized (Windows)
  ramlord --demo                            run the app on the sample building (Windows)
  ramlord --building FILE --render OUT.png  draw one tower frame from a Building file
  ramlord --building FILE --card OUT.png    draw the rent roll card
  ramlord --snapshot FILE --render OUT.png  group a Snapshot with the rules, then draw
  ramlord --dump OUT.json                   write one live Snapshot (Windows)
  ramlord --self-test OUT.json              sample and draw off screen for 10 seconds and
                                            check the budgets (Windows)";

/// Every option; each may appear once.
#[derive(Default)]
struct Options {
    building: Option<PathBuf>,
    snapshot: Option<PathBuf>,
    render: Option<PathBuf>,
    card: Option<PathBuf>,
    dump: Option<PathBuf>,
    self_test: Option<PathBuf>,
    minimized: bool,
    demo: bool,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, UsageError> {
    let mut args = args.into_iter();
    let mut o = Options::default();

    fn take(
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
    fn set(flag: &mut bool, name: &'static str) -> Result<(), UsageError> {
        if std::mem::replace(flag, true) {
            return Err(UsageError::Repeated(name));
        }
        Ok(())
    }

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--building" => take(&mut o.building, "--building", args.next())?,
            "--snapshot" => take(&mut o.snapshot, "--snapshot", args.next())?,
            "--render" => take(&mut o.render, "--render", args.next())?,
            "--card" => take(&mut o.card, "--card", args.next())?,
            "--dump" => take(&mut o.dump, "--dump", args.next())?,
            "--self-test" => take(&mut o.self_test, "--self-test", args.next())?,
            "--minimized" => set(&mut o.minimized, "--minimized")?,
            "--demo" => set(&mut o.demo, "--demo")?,
            _ => return Err(UsageError::Unknown(arg)),
        }
    }

    let drawing = o.render.is_some() || o.card.is_some();
    let modes = [
        o.building.is_some() || o.snapshot.is_some() || drawing,
        o.dump.is_some(),
        o.self_test.is_some(),
        o.minimized,
        o.demo,
    ];
    if modes.iter().filter(|&&m| m).count() > 1 || (o.building.is_some() && o.snapshot.is_some()) {
        return Err(UsageError::Conflict);
    }
    let (render, card) = (o.render, o.card);
    Ok(match (o.building, o.snapshot) {
        (Some(file), None) if drawing => Command::Building { file, render, card },
        (None, Some(file)) if drawing => Command::Snapshot { file, render, card },
        (Some(_), _) | (_, Some(_)) => return Err(UsageError::Incomplete),
        (None, None) if drawing => return Err(UsageError::Incomplete),
        (None, None) => {
            if let Some(out) = o.dump {
                Command::Dump(out)
            } else if let Some(out) = o.self_test {
                Command::SelfTest(out)
            } else if o.demo {
                Command::Demo
            } else {
                Command::Run {
                    minimized: o.minimized,
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(line: &str) -> Result<Command, UsageError> {
        parse(line.split_whitespace().map(String::from))
    }

    #[test]
    fn no_arguments_runs_the_app() {
        assert_eq!(parse_str(""), Ok(Command::Run { minimized: false }));
        assert_eq!(
            parse_str("--minimized"),
            Ok(Command::Run { minimized: true })
        );
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
    }

    #[test]
    fn snapshot_dump_and_self_test() {
        assert_eq!(
            parse_str("--snapshot s.json --render t.png --card c.png"),
            Ok(Command::Snapshot {
                file: "s.json".into(),
                render: Some("t.png".into()),
                card: Some("c.png".into())
            })
        );
        assert_eq!(
            parse_str("--dump out.json"),
            Ok(Command::Dump("out.json".into()))
        );
        assert_eq!(
            parse_str("--self-test out.json"),
            Ok(Command::SelfTest("out.json".into()))
        );
    }

    #[test]
    fn usage_errors() {
        assert_eq!(parse_str("--building b.json"), Err(UsageError::Incomplete));
        assert_eq!(parse_str("--snapshot s.json"), Err(UsageError::Incomplete));
        assert_eq!(parse_str("--render t.png"), Err(UsageError::Incomplete));
        assert_eq!(
            parse_str("--building"),
            Err(UsageError::MissingValue("--building"))
        );
        assert_eq!(parse_str("--dump"), Err(UsageError::MissingValue("--dump")));
        assert_eq!(
            parse_str("--building a --building b --render t.png"),
            Err(UsageError::Repeated("--building"))
        );
        assert_eq!(
            parse_str("--demo --demo"),
            Err(UsageError::Repeated("--demo"))
        );
        assert_eq!(
            parse_str("--building b.json --snapshot s.json --render t.png"),
            Err(UsageError::Conflict)
        );
        assert_eq!(
            parse_str("--demo --building b.json --render t.png"),
            Err(UsageError::Conflict)
        );
        assert_eq!(parse_str("--demo --minimized"), Err(UsageError::Conflict));
        assert_eq!(
            parse_str("--dump a.json --self-test b.json"),
            Err(UsageError::Conflict)
        );
        assert_eq!(
            parse_str("--dump a.json --render t.png"),
            Err(UsageError::Conflict)
        );
        assert_eq!(
            parse_str("--frobnicate"),
            Err(UsageError::Unknown("--frobnicate".into()))
        );
    }
}
