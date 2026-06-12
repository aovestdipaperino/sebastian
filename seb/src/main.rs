//! `seb`: renders a mermaid diagram (.mmd) to SVG, mmdc-style.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut id = "my-svg".to_owned();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-i" | "--input" => {
                i += 1;
                input = args.get(i).cloned();
            }
            "-o" | "--output" => {
                i += 1;
                output = args.get(i).cloned();
            }
            "--id" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    id.clone_from(v);
                }
            }
            other => {
                eprintln!("unknown argument: {other}");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    let Some(input) = input else {
        eprintln!("usage: seb -i input.mmd [-o output.svg] [--id svg-id]");
        return ExitCode::FAILURE;
    };

    let source = match std::fs::read_to_string(&input) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("failed to read {input}: {err}");
            return ExitCode::FAILURE;
        }
    };

    match sebastian::render::render_diagram(&source, &id) {
        Ok(svg) => {
            if let Some(output) = output {
                if let Err(err) = std::fs::write(&output, svg) {
                    eprintln!("failed to write {output}: {err}");
                    return ExitCode::FAILURE;
                }
            } else {
                println!("{svg}");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
