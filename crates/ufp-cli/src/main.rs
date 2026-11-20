//! `ufp` — command-line driver for Uncle Frank's Plan.

use std::{collections::BTreeMap, env, fs, process::ExitCode};

fn usage() {
    eprintln!("usage: ufp <tokens|parse|run|compile> <file.ufp> [--set name=text] | ufp compile <file.ufp> <output>");
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
        "run" => run(&path, &source, args),
        "compile" => compile(&path, &source, args),
        _ => {
            usage();
            ExitCode::from(2)
        }
    }
}

fn compile(path: &str, source: &str, mut args: impl Iterator<Item = String>) -> ExitCode {
    let Some(output) = args.next() else {
        eprintln!("ufp: compile requires output path");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("ufp: compile accepts one output path");
        return ExitCode::from(2);
    }
    // Matrix programs use the deliberately narrow typed-AST ggml path. Keep
    // the existing shallow text-plan parser/emitter as the default path.
    if let Ok(ast) = ufp_syntax::parse_ast(source) {
        if ast.declarations.iter().any(|n| {
            matches!(
                n.kind,
                ufp_syntax::SyntaxKind::Matrix | ufp_syntax::SyntaxKind::Procedure
            )
        }) {
            let ggml_source = env::var_os("UFP_GGML_SOURCE_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("third_party/ggml"));
            let ggml_build = env::var_os("UFP_GGML_BUILD_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from(".cache/ggml-build"));
            return match ufp_codegen::compile_ggml_matmul_ufp(
                source,
                std::path::Path::new(&output),
                &ggml_source,
                &ggml_build,
            ) {
                Ok(()) => {
                    println!("compiled {output}");
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("{path}: compile: {error}");
                    ExitCode::from(1)
                }
            };
        }
    }
    let plan = match ufp_syntax::parse_plan(source) {
        Ok(plan) => plan,
        Err(diagnostics) => return print_errors(path, &diagnostics),
    };
    match ufp_codegen::compile_text_plan(&plan, std::path::Path::new(&output)) {
        Ok(()) => {
            println!("compiled {output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{path}: compile: {error}");
            ExitCode::from(1)
        }
    }
}

fn run(path: &str, source: &str, args: impl Iterator<Item = String>) -> ExitCode {
    let plan = match ufp_syntax::parse_plan(source) {
        Ok(plan) => plan,
        Err(diagnostics) => return print_errors(path, &diagnostics),
    };
    let mut overrides = BTreeMap::new();
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        if arg != "--set" {
            eprintln!("ufp: unknown run option `{arg}`");
            return ExitCode::from(2);
        }
        let Some(binding) = args.next() else {
            eprintln!("ufp: --set requires name=text");
            return ExitCode::from(2);
        };
        let Some((name, value)) = binding.split_once('=') else {
            eprintln!("ufp: --set requires name=text");
            return ExitCode::from(2);
        };
        overrides.insert(name.to_owned(), value.to_owned());
    }
    match ufp_runtime::run_text_plan(&plan, &overrides) {
        Ok(result) => {
            for (name, text) in result.outputs {
                println!("{name}: {text}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{path}: runtime: {}", error.0);
            ExitCode::from(1)
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
