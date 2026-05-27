// zigrun — a compiler for a subset of Zig, written in Rust. It lowers the source
// to C (codegen.rs), invokes `cc` to produce a native executable, and runs it.
// The program's `pub fn main() u8` return value becomes the process exit code,
// which is the oracle's observable result (oracle/check.sh).
//
//   zigrun emit-c FILE.zig   # print the generated C
//   zigrun run    FILE.zig   # compile (via C + cc) and execute

mod ast;
mod codegen;
mod lexer;
mod parser;

use std::env;
use std::fs;
use std::process::{self, Command};

use lexer::Lexer;
use parser::Parser;

/// Sentinel exit for compile/runtime errors, kept OUT of the u8 result range so
/// an erroring program can never be mistaken for one that returned a valid value.
const ERR_EXIT: i32 = 101;

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(String::as_str);
    let path = match (cmd, args.get(2)) {
        (Some("run"), Some(p)) | (Some("emit-c"), Some(p)) => p.clone(),
        _ => {
            eprintln!("usage: zigrun <run|emit-c> <file.zig>");
            process::exit(2);
        }
    };

    let source = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("zigrun: cannot read {path}: {e}");
        process::exit(2);
    });

    let c_src = match compile_to_c(&source) {
        Ok(c) => c,
        Err(err) => {
            eprintln!("zigrun: {err}");
            process::exit(ERR_EXIT);
        }
    };

    if cmd == Some("emit-c") {
        print!("{c_src}");
        return;
    }
    process::exit(compile_c_and_run(&c_src, &path));
}

fn compile_to_c(source: &str) -> Result<String, String> {
    let tokens = Lexer::new(source).tokenize()?;
    let program = Parser::new(tokens).parse_program()?;
    codegen::emit_c(&program)
}

/// Writes the C, compiles it with `cc` (or $CC), runs the binary, and returns its
/// exit code. Compilation/exec failures return the error sentinel.
fn compile_c_and_run(c_src: &str, src_path: &str) -> i32 {
    let dir = env::temp_dir();
    let stamp = process::id();
    let cfile = dir.join(format!("zigrun-{stamp}.c"));
    let binfile = dir.join(format!("zigrun-{stamp}.bin"));

    if let Err(e) = fs::write(&cfile, c_src) {
        eprintln!("zigrun: writing C: {e}");
        return ERR_EXIT;
    }

    let cc = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let compiled = Command::new(&cc)
        .arg(&cfile)
        .arg("-std=c11")
        .arg("-O0")
        .arg("-o")
        .arg(&binfile)
        .status();
    let _ = fs::remove_file(&cfile);
    match compiled {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("zigrun: cc failed to compile {src_path} (status {s})");
            let _ = fs::remove_file(&binfile);
            return ERR_EXIT;
        }
        Err(e) => {
            eprintln!("zigrun: cannot invoke `{cc}`: {e}");
            return ERR_EXIT;
        }
    }

    let ran = Command::new(&binfile).status();
    let _ = fs::remove_file(&binfile);
    match ran {
        Ok(s) => s.code().unwrap_or(ERR_EXIT),
        Err(e) => {
            eprintln!("zigrun: cannot execute compiled binary: {e}");
            ERR_EXIT
        }
    }
}
