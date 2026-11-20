//! `ufp` — command-line driver for Uncle Frank's Plan.

use std::{env, fs, process::ExitCode};

fn usage() {
    eprintln!("usage: ufp <tokens|parse> <file.ufp>");
}

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let Some(path) = args.next() else {
        usage();
        return ExitCode::from(2);
    };
    let Ok(source) = fs::read_to_string(&path) else {
        eprintln!("ufp: cannot read {path}");
        return ExitCode::from(1);
    };
    match command.as_str() {
        "tokens" => {
            let (tokens, diagnostics) = ufp_syntax::lex(&source);
            for t in tokens {
                println!(
                    "{:?} {}:{}:{} {:?}",
                    t.kind, t.span.start, t.span.line, t.span.column, t.text
                );
            }
            print_errors(&path, &diagnostics)
        }
        "parse" => match ufp_syntax::parse_plan(&source) {
            Ok(plan) => {
                println!("{plan:#?}");
                ExitCode::SUCCESS
            }
            Err(diagnostics) => print_errors(&path, &diagnostics),
        },
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}
fn print_errors(path: &str, diagnostics: &[ufp_syntax::Diagnostic]) -> ExitCode {
    for d in diagnostics {
        eprintln!("{path}:{d}");
    }
    if diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}
