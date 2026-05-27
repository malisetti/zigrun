mod ast;
mod interp;
mod lexer;
mod parser;

use std::env;
use std::fs;
use std::process;

use lexer::Lexer;
use parser::Parser;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 || args[1] != "run" {
        eprintln!("usage: zigrun run <file.zig>");
        process::exit(2);
    }

    let path = &args[2];
    let source = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("failed to read {path}: {e}");
        process::exit(1);
    });

    match compile_and_run(&source) {
        Ok(code) => process::exit(code as i32),
        Err(err) => {
            eprintln!("{err}");
            // Sentinel exit for compile/runtime errors, kept OUT of the oracle's
            // result range so an erroring program can never be mistaken for one
            // that returned a valid value (e.g. ifelse legitimately returns 1).
            process::exit(101);
        }
    }
}

fn compile_and_run(source: &str) -> Result<u8, String> {
    let tokens = Lexer::new(source).tokenize()?;
    let program = Parser::new(tokens).parse_program()?;
    interp::run(&program)
}
