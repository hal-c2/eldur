//! Command-line workflow and file-system adapters for `eldur`.
//!
//! All subcommand logic lives in this library behind the [`Converter`] trait so
//! integration tests can exercise conversion, caching, file watching, and QML
//! validation end-to-end without depending on unfinished converter stages.

mod check_cmd;
mod convert_cmd;
mod resources;
mod watch_cmd;

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::bail;
use clap::{Args, Parser, Subcommand, ValueEnum};

pub use check_cmd::{QmlTools, check_dir_with_tools};
pub use convert_cmd::{
    CONVERTER_FORMAT_VERSION, ConvertOutcome, ConvertStatus, default_output_dir, serialize_options,
};
pub use resources::FsResources;
pub use watch_cmd::{WatchChannel, WatchStop};

/// Top-level `eldur` command-line parser.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "eldur",
    version,
    about = "Convert static HTML5 + CSS + JS pages into runnable Qt Quick (QML) applications"
)]
pub struct Cli {
    /// Subcommand to execute.
    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands supported by `eldur`.
#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Convert an HTML document to QML, using the build cache when fresh.
    Convert(ConvertArgs),
    /// Convert an HTML document and re-convert whenever any dependency changes.
    Watch(ConvertArgs),
    /// Validate `.qml` files with `qmllint` and run `Main.qml` off-screen in `qml6`.
    Check(CheckArgs),
}

/// Root QML component selection for `--root`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum RootArg {
    /// Emit a runnable `ApplicationWindow`.
    #[default]
    Window,
    /// Emit an embeddable `Item`.
    Item,
}

impl From<RootArg> for eldur_convert::RootKind {
    fn from(value: RootArg) -> Self {
        match value {
            RootArg::Window => Self::Window,
            RootArg::Item => Self::Item,
        }
    }
}

/// Arguments shared by `eldur convert` and `eldur watch`.
#[derive(Debug, Clone, Args)]
pub struct ConvertArgs {
    /// Path to the entry HTML document.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    /// Output directory (defaults to `<input stem>_qml/`).
    #[arg(short = 'o', long = "output", value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Conversion viewport as `WxH` in logical pixels (default `1280x800`).
    #[arg(long = "viewport", value_name = "WxH", value_parser = parse_viewport)]
    pub viewport: Option<(f32, f32)>,

    /// Root QML element kind.
    #[arg(long = "root", value_enum, default_value_t = RootArg::Window)]
    pub root: RootArg,

    /// Omit HTML source comments in generated QML.
    #[arg(long = "no-comments")]
    pub no_comments: bool,

    /// Ignore dynamic pseudo-class state variants (`:hover`, `:focus`, ...).
    #[arg(long = "no-states")]
    pub no_states: bool,

    /// Ignore CSS transitions and keyframe animations.
    #[arg(long = "no-animations")]
    pub no_animations: bool,

    /// Bypass reading from and writing to the build cache.
    #[arg(long = "no-cache")]
    pub no_cache: bool,

    /// Build cache directory (defaults to `<out>/.eldur-cache`).
    #[arg(long = "cache-dir", value_name = "DIR")]
    pub cache_dir: Option<PathBuf>,

    /// Suppress `cached` / `converted` timing lines.
    #[arg(short = 'q', long = "quiet")]
    pub quiet: bool,
}

impl ConvertArgs {
    /// Constructs default conversion arguments for `input`.
    #[must_use]
    pub fn new(input: impl Into<PathBuf>) -> Self {
        Self {
            input: input.into(),
            output: None,
            viewport: None,
            root: RootArg::Window,
            no_comments: false,
            no_states: false,
            no_animations: false,
            no_cache: false,
            cache_dir: None,
            quiet: false,
        }
    }

    /// Output directory after applying the `<input stem>_qml/` default.
    #[must_use]
    pub fn resolved_output_dir(&self) -> PathBuf {
        self.output
            .clone()
            .unwrap_or_else(|| default_output_dir(&self.input))
    }

    /// Cache directory after applying the `<out>/.eldur-cache` default.
    #[must_use]
    pub fn resolved_cache_dir(&self, out_dir: &Path) -> PathBuf {
        self.cache_dir
            .clone()
            .unwrap_or_else(|| out_dir.join(".eldur-cache"))
    }

    /// Translates CLI flags into [`eldur_convert::Options`].
    #[must_use]
    pub fn to_convert_options(&self) -> eldur_convert::Options {
        let mut viewport = eldur_convert::Viewport::default();
        if let Some((width, height)) = self.viewport {
            viewport.width = width;
            viewport.height = height;
        }
        eldur_convert::Options {
            viewport,
            root: self.root.into(),
            source_comments: !self.no_comments,
            state_variants: !self.no_states,
            animations: !self.no_animations,
        }
    }
}

/// Arguments for `eldur check`.
#[derive(Debug, Clone, Args)]
pub struct CheckArgs {
    /// Directory containing generated `.qml` files to validate.
    #[arg(value_name = "DIR")]
    pub dir: PathBuf,
}

/// Parses a `WxH` viewport specification such as `1280x800` or `375x667`.
pub fn parse_viewport(spec: &str) -> Result<(f32, f32), String> {
    let mut parts = spec.split(['x', 'X']);
    let (Some(w_str), Some(h_str), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(format!(
            "invalid viewport '{spec}': expected WxH (for example 1280x800)"
        ));
    };
    let width: f32 = w_str
        .parse()
        .map_err(|_| format!("invalid viewport width '{w_str}' in '{spec}'"))?;
    let height: f32 = h_str
        .parse()
        .map_err(|_| format!("invalid viewport height '{h_str}' in '{spec}'"))?;

    if !width.is_finite() || width <= 0.0 || !height.is_finite() || height <= 0.0 {
        return Err(format!(
            "invalid viewport '{spec}': width and height must be positive finite numbers"
        ));
    }
    Ok((width, height))
}

/// Pluggable HTML-to-QML conversion backend.
pub trait Converter {
    /// Converts `input` under `options`, reading sub-resources from `resources`.
    fn convert(
        &self,
        input: &eldur_convert::Input,
        options: &eldur_convert::Options,
        resources: &mut dyn eldur_convert::Resources,
    ) -> eldur_convert::Output;
}

impl<F> Converter for F
where
    F: Fn(
        &eldur_convert::Input,
        &eldur_convert::Options,
        &mut dyn eldur_convert::Resources,
    ) -> eldur_convert::Output,
{
    fn convert(
        &self,
        input: &eldur_convert::Input,
        options: &eldur_convert::Options,
        resources: &mut dyn eldur_convert::Resources,
    ) -> eldur_convert::Output {
        (self)(input, options, resources)
    }
}

/// Production [`Converter`] delegating directly to [`eldur_convert::convert`].
#[derive(Debug, Clone, Copy, Default)]
pub struct RealConverter;

impl Converter for RealConverter {
    fn convert(
        &self,
        input: &eldur_convert::Input,
        options: &eldur_convert::Options,
        resources: &mut dyn eldur_convert::Resources,
    ) -> eldur_convert::Output {
        eldur_convert::convert(input, options, resources)
    }
}

/// Executes any parsed [`Cli`] command against `converter` using standard I/O.
pub fn run(cli: Cli, converter: &dyn Converter) -> anyhow::Result<()> {
    match cli.command {
        Command::Convert(args) => {
            run_convert(&args, converter)?;
            Ok(())
        }
        Command::Watch(args) => run_watch(&args, converter),
        Command::Check(args) => run_check(&args),
    }
}

/// Executes `eldur convert` writing status to stdout and diagnostics to stderr.
pub fn run_convert(
    args: &ConvertArgs,
    converter: &dyn Converter,
) -> anyhow::Result<ConvertOutcome> {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    run_convert_with_io(args, converter, &mut stdout, &mut stderr)
}

/// Executes `eldur convert` writing status and diagnostics to caller-supplied streams.
pub fn run_convert_with_io(
    args: &ConvertArgs,
    converter: &dyn Converter,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> anyhow::Result<ConvertOutcome> {
    convert_cmd::execute_convert(args, converter, stdout, stderr)
}

/// Executes `eldur watch` until interrupted.
pub fn run_watch(args: &ConvertArgs, converter: &dyn Converter) -> anyhow::Result<()> {
    let (channel, _stop) = WatchChannel::new();
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    run_watch_with_channel(args, converter, channel, &mut stdout, &mut stderr, None)
}

/// Executes `eldur watch` with an explicit [`WatchChannel`] and optional
/// per-cycle observer callback (for testing).
pub fn run_watch_with_channel(
    args: &ConvertArgs,
    converter: &dyn Converter,
    channel: WatchChannel,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    on_cycle: Option<&mut dyn FnMut(&ConvertOutcome)>,
) -> anyhow::Result<()> {
    watch_cmd::execute_watch(args, converter, channel, stdout, stderr, on_cycle)
}

/// Executes `eldur check` using `qmllint` and `qml6` from `PATH`.
///
/// Validates all `.qml` files recursively under `args.dir` with `qmllint` and
/// loads every `Main.qml` under `args.dir` in `qml6` off-screen for 3 s, failing
/// on any diagnostic output or early non-zero exit.
pub fn run_check(args: &CheckArgs) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    let ok = check_dir_with_tools(&args.dir, &QmlTools::default(), &mut stdout)?;
    if !ok {
        bail!("QML validation failed for {}", args.dir.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_parser_accepts_valid_and_rejects_invalid_specs() {
        assert_eq!(parse_viewport("1280x800"), Ok((1280.0, 800.0)));
        assert_eq!(parse_viewport("375X667"), Ok((375.0, 667.0)));
        assert_eq!(parse_viewport("800.5x600.25"), Ok((800.5, 600.25)));

        for bad in [
            "",
            "1280",
            "1280x",
            "x800",
            "1280x800x2",
            "0x800",
            "1280x0",
            "-100x200",
            "NaNx800",
            "infx800",
            "abcxdef",
        ] {
            assert!(
                parse_viewport(bad).is_err(),
                "expected viewport '{bad}' to fail"
            );
        }
    }

    #[test]
    fn default_output_dir_uses_input_stem_next_to_input() {
        assert_eq!(
            default_output_dir(Path::new("page.html")),
            PathBuf::from("page_qml")
        );
        assert_eq!(
            default_output_dir(Path::new("examples/card.html")),
            PathBuf::from("examples/card_qml")
        );
        assert_eq!(
            default_output_dir(Path::new("/tmp/site/index.html")),
            PathBuf::from("/tmp/site/index_qml")
        );
    }
}
