//! How `convert` and `watch` report diagnostics and the outcome: as lines for
//! a person, or as JSON objects, one per line on stdout, for a program.
//!
//! A program cannot split the human line `warning[css]: source:line: message`
//! reliably: a source that is a URL holds `:` itself.

use std::fmt;
use std::io::Write;
use std::path::{Component, Path};
use std::time::Duration;

use anyhow::Context;
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::ConvertStatus;

/// The form `--message-format` selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum MessageFormat {
    /// Diagnostics as `severity[stage]: source:line: message` lines on
    /// stderr, the outcome as a `converted`/`cached` line on stdout.
    #[default]
    Human,
    /// One JSON object per line on stdout: a `diagnostic` for each
    /// diagnostic, then `done`, or `error` when the conversion failed.
    Json,
}

/// A conversion diagnostic as the build cache keeps it and
/// `--message-format json` reports it: the fields of
/// [`eldur_convert::Diagnostic`], owned, so a cached run replays them whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticRecord {
    /// How serious it is.
    #[serde(with = "severity_name")]
    pub severity: eldur_convert::Severity,
    /// `html`, `css`, `js` or `qml`.
    pub stage: String,
    /// The file or URL the diagnostic is about.
    pub source: String,
    /// 1-based line in `source`, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// What is wrong.
    pub message: String,
}

impl From<&eldur_convert::Diagnostic> for DiagnosticRecord {
    fn from(diag: &eldur_convert::Diagnostic) -> Self {
        Self {
            severity: diag.severity,
            stage: diag.stage.to_owned(),
            source: diag.source.clone(),
            line: diag.line,
            message: diag.message.clone(),
        }
    }
}

/// The human line, the same as [`eldur_convert::Diagnostic`]'s.
impl fmt::Display for DiagnosticRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}[{}]: {}",
            severity_name::of(self.severity),
            self.stage,
            self.source
        )?;
        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }
        write!(f, ": {}", self.message)
    }
}

mod severity_name {
    use eldur_convert::Severity;
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub(super) fn of(severity: Severity) -> &'static str {
        match severity {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }

    pub(super) fn serialize<S: Serializer>(
        severity: &Severity,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(of(*severity))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Severity, D::Error> {
        let name = String::deserialize(deserializer)?;
        match name.as_str() {
            "info" => Ok(Severity::Info),
            "warning" => Ok(Severity::Warning),
            "error" => Ok(Severity::Error),
            other => Err(D::Error::custom(format!("unknown severity '{other}'"))),
        }
    }
}

/// One line of `--message-format json`; `type` is its first key, so a
/// reader can branch on it before reading the rest.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Message<'a> {
    Diagnostic(&'a DiagnosticRecord),
    Done {
        status: &'static str,
        output: String,
        files: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        main: Option<String>,
    },
    Error {
        message: String,
    },
}

/// The diagnostics as the build cache stores them: one JSON object each, so
/// a hit replays every field and not only the human line.
pub(crate) fn encode_cached(diagnostics: &[DiagnosticRecord]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diag| serde_json::to_string(diag).expect("a diagnostic serialises"))
        .collect()
}

/// The diagnostics of a cache entry, or `None` when one does not read as a
/// record (an entry written in another shape), which makes the entry a miss.
pub(crate) fn decode_cached(stored: &[String]) -> Option<Vec<DiagnosticRecord>> {
    stored
        .iter()
        .map(|diag| serde_json::from_str(diag).ok())
        .collect()
}

/// What a finished conversion reports.
pub(crate) struct Report<'a, I: fmt::Display> {
    pub(crate) status: ConvertStatus,
    /// The input as the human line shows it.
    pub(crate) input: I,
    pub(crate) output_dir: &'a Path,
    /// Every file of the output, relative to `output_dir`.
    pub(crate) files: Vec<&'a Path>,
    pub(crate) diagnostics: &'a [DiagnosticRecord],
    pub(crate) elapsed: Duration,
}

impl<I: fmt::Display> Report<'_, I> {
    /// Writes the diagnostics and the outcome in `format`. `quiet` leaves
    /// out the human `converted`/`cached` line; the JSON `done` line, which
    /// a program waits for, is always written.
    pub(crate) fn write(
        &self,
        format: MessageFormat,
        quiet: bool,
        stdout: &mut dyn Write,
        stderr: &mut dyn Write,
    ) -> anyhow::Result<()> {
        let status = match self.status {
            ConvertStatus::Converted => "converted",
            ConvertStatus::Cached => "cached",
        };
        match format {
            MessageFormat::Human => {
                for diag in self.diagnostics {
                    writeln!(stderr, "{diag}").context("failed to write diagnostic to stderr")?;
                }
                if !quiet {
                    writeln!(
                        stdout,
                        "{status} {} -> {} ({:.2?})",
                        self.input,
                        self.output_dir.display(),
                        self.elapsed
                    )
                    .context("failed to write status to stdout")?;
                }
            }
            MessageFormat::Json => {
                for diag in self.diagnostics {
                    write_json(stdout, &Message::Diagnostic(diag))?;
                }
                let files: Vec<String> = self.files.iter().map(|f| slash_path(f)).collect();
                let main = files.iter().find(|f| *f == "Main.qml").cloned();
                write_json(
                    stdout,
                    &Message::Done {
                        status,
                        output: self.output_dir.to_string_lossy().into_owned(),
                        files,
                        main,
                    },
                )?;
            }
        }
        Ok(())
    }
}

/// Reports a failed conversion: `error: …` on stderr, or an `error` object
/// on stdout.
pub(crate) fn write_error(
    format: MessageFormat,
    err: &anyhow::Error,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> anyhow::Result<()> {
    match format {
        MessageFormat::Human => {
            writeln!(stderr, "error: {err:#}").context("failed to write error to stderr")
        }
        MessageFormat::Json => write_json(
            stdout,
            &Message::Error {
                message: format!("{err:#}"),
            },
        ),
    }
}

fn write_json(stdout: &mut dyn Write, message: &Message<'_>) -> anyhow::Result<()> {
    let line = serde_json::to_string(message).context("failed to serialise a message")?;
    writeln!(stdout, "{line}").context("failed to write message to stdout")?;
    // A program reading a pipe acts on each line as it comes, `watch`'s
    // above all.
    stdout.flush().context("failed to flush stdout")
}

/// `path` with `/` between its parts on every platform, as a program that
/// joins it to `output` expects.
fn slash_path(path: &Path) -> String {
    let parts: Vec<_> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy()),
            _ => None,
        })
        .collect();
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use eldur_convert::{Diagnostic, Severity};

    use super::*;

    #[test]
    fn the_human_line_is_the_converters() {
        for severity in [Severity::Info, Severity::Warning, Severity::Error] {
            for line in [Some(7), None] {
                let diag = Diagnostic {
                    severity,
                    stage: "js",
                    source: "https://example.com/app.js".to_owned(),
                    line,
                    message: "eval is not supported".to_owned(),
                };
                assert_eq!(DiagnosticRecord::from(&diag).to_string(), diag.to_string());
            }
        }
    }

    #[test]
    fn a_cached_record_reads_back_whole_and_a_human_line_does_not() {
        let records = vec![
            DiagnosticRecord {
                severity: Severity::Error,
                stage: "qml".to_owned(),
                source: "Main.qml".to_owned(),
                line: Some(12),
                message: "x".to_owned(),
            },
            DiagnosticRecord {
                severity: Severity::Info,
                stage: "html".to_owned(),
                source: "a:b".to_owned(),
                line: None,
                message: "y".to_owned(),
            },
        ];
        assert_eq!(decode_cached(&encode_cached(&records)), Some(records));
        assert_eq!(
            decode_cached(&["warning[css]: style.css:3: unsupported filter".to_owned()]),
            None
        );
    }
}
