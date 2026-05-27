// zigrun — a from-scratch interpreter for a SUBSET of Zig, built in Rust.
//
// Contract (see oracle/ and README.md): `zigrun run FILE.zig` evaluates the
// program and the return value of `pub fn main() u8` becomes the process exit
// code. That exit code is the EXTERNAL ORACLE — oracle/check.sh runs each
// oracle/*.zig and compares the real exit code to oracle/<name>.exit. A worker
// cannot make the oracle green by passing its own unit tests; the program must
// actually run to the right answer.
//
// This is the oracle-first RED skeleton. The orchestration's job is to replace
// the unimplemented pipeline below with lex -> parse -> sema -> interpret until
// the oracle goes green, one feature slice at a time.

use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args[1] != "run" {
        eprintln!("usage: zigrun run <file.zig>");
        exit(2);
    }
    let path = &args[2];
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("zigrun: cannot read {path}: {e}");
            exit(2);
        }
    };
    let _ = src; // pipeline not implemented yet

    // RED until built: anything that is not "0" tells the oracle this slice is
    // unimplemented. Implementers replace this with the real interpreter whose
    // main() return value is propagated here as the exit code.
    eprintln!("zigrun: compiler pipeline not implemented (oracle is RED until built)");
    exit(99);
}
