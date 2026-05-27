// C backend for zigrun: lowers the Zig-subset AST to C source. zigrun is a real
// compiler — it emits C, which `cc` compiles to a native executable; the
// program's `main() u8` becomes the process exit code. (Previously zigrun
// tree-walked the AST; now it generates code.)
//
// u8 semantics here are C's `uint8_t` (wrapping), a known divergence from Zig's
// checked arithmetic — tracked in FEATURES.md.

use crate::ast::{BinOp, Expr, Function, Program, Stmt};
use std::fmt::Write;

pub fn emit_c(program: &Program) -> Result<String, String> {
    if !program.functions.iter().any(|f| f.name == "main") {
        return Err("no `main` function".to_string());
    }
    let mut out = String::new();
    out.push_str("#include <stdint.h>\n\n");

    // Forward declarations so functions may call each other / recurse.
    for f in &program.functions {
        let _ = writeln!(out, "{};", prototype(f));
    }
    out.push('\n');

    for f in &program.functions {
        emit_function(&mut out, f)?;
        out.push('\n');
    }

    // C entry point: Zig `pub fn main() u8` → process exit code.
    out.push_str("int main(void) {\n    return (int)zig_main();\n}\n");
    Ok(out)
}

/// C-safe name: `main` is renamed (the C entry wrapper owns `main`), and every
/// user function is prefixed to avoid clashing with libc symbols.
fn c_fn(name: &str) -> String {
    format!("zig_{name}")
}

fn prototype(f: &Function) -> String {
    let params = if f.params.is_empty() {
        "void".to_string()
    } else {
        f.params
            .iter()
            .map(|p| format!("uint8_t {p}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("uint8_t {}({})", c_fn(&f.name), params)
}

fn emit_function(out: &mut String, f: &Function) -> Result<(), String> {
    let _ = writeln!(out, "{} {{", prototype(f));
    for s in &f.body {
        emit_stmt(out, s, 1)?;
    }
    out.push_str("}\n");
    Ok(())
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn emit_stmt(out: &mut String, stmt: &Stmt, depth: usize) -> Result<(), String> {
    indent(out, depth);
    match stmt {
        Stmt::Let { name, value } => {
            let _ = writeln!(out, "uint8_t {name} = {};", emit_expr(value)?);
        }
        Stmt::Assign { name, value } => {
            let _ = writeln!(out, "{name} = {};", emit_expr(value)?);
        }
        Stmt::Return(e) => {
            let _ = writeln!(out, "return {};", emit_expr(e)?);
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let _ = writeln!(out, "if ({}) {{", emit_expr(cond)?);
            for s in then_branch {
                emit_stmt(out, s, depth + 1)?;
            }
            indent(out, depth);
            out.push('}');
            if let Some(eb) = else_branch {
                out.push_str(" else {\n");
                for s in eb {
                    emit_stmt(out, s, depth + 1)?;
                }
                indent(out, depth);
                out.push('}');
            }
            out.push('\n');
        }
        Stmt::While { cond, body } => {
            let _ = writeln!(out, "while ({}) {{", emit_expr(cond)?);
            for s in body {
                emit_stmt(out, s, depth + 1)?;
            }
            indent(out, depth);
            out.push_str("}\n");
        }
    }
    Ok(())
}

fn emit_expr(expr: &Expr) -> Result<String, String> {
    Ok(match expr {
        Expr::Int(n) => n.to_string(),
        Expr::Var(name) => name.clone(),
        Expr::Call { name, args } => {
            let mut parts = Vec::with_capacity(args.len());
            for a in args {
                parts.push(emit_expr(a)?);
            }
            format!("{}({})", c_fn(name), parts.join(", "))
        }
        Expr::BinOp { op, left, right } => {
            format!("({} {} {})", emit_expr(left)?, c_op(*op), emit_expr(right)?)
        }
    })
}

fn c_op(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn c_of(src: &str) -> String {
        let toks = Lexer::new(src).tokenize().unwrap();
        let prog = Parser::new(toks).parse_program().unwrap();
        emit_c(&prog).unwrap()
    }

    #[test]
    fn emits_recursion_and_entry() {
        let c = c_of("fn fib(n: u8) u8 { if (n < 2) { return n; } return fib(n - 1) + fib(n - 2); } pub fn main() u8 { return fib(10); }");
        assert!(c.contains("uint8_t zig_fib(uint8_t n)"));
        assert!(c.contains("zig_fib((n - 1))"));
        assert!(c.contains("int main(void)"));
        assert!(c.contains("return (int)zig_main();"));
    }
}
