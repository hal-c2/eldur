use std::io;
use std::process::ExitCode;

use clap::Parser;
use eldur_cli::{
    Cli, Command, QmlTools, RealConverter, check_dir_with_tools, run_convert, run_watch,
};

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Command::Convert(args) => match run_convert(&args, &RealConverter) {
            Ok(_) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err:#}");
                ExitCode::from(1)
            }
        },
        Command::Watch(args) => match run_watch(&args, &RealConverter) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                eprintln!("error: {err:#}");
                ExitCode::from(1)
            }
        },
        Command::Check(args) => {
            let mut stdout = io::stdout();
            match check_dir_with_tools(&args.dir, &QmlTools::default(), &mut stdout) {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::from(1),
                Err(err) => {
                    eprintln!("error: {err:#}");
                    ExitCode::from(1)
                }
            }
        }
    }
}
