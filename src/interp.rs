// Tree-walking interpreter for the zigrun Zig subset. All values are u8 (the
// subset's only type); comparisons yield 1/0 and if/while test "non-zero".
// `run` invokes `main` and returns its u8 — which main.rs propagates as the
// process exit code (the oracle's observable result).

use crate::ast::{BinOp, Expr, Function, Program, Stmt};
use std::collections::HashMap;

type Funcs<'a> = HashMap<&'a str, &'a Function>;

pub fn run(program: &Program) -> Result<u8, String> {
    let funcs: Funcs = program.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    let main = funcs.get("main").ok_or("no `main` function")?;
    call(main, &[], &funcs)
}

fn call(func: &Function, args: &[u8], funcs: &Funcs) -> Result<u8, String> {
    if args.len() != func.params.len() {
        return Err(format!(
            "function `{}` expects {} argument(s), got {}",
            func.name,
            func.params.len(),
            args.len()
        ));
    }
    let mut env: HashMap<String, u8> = HashMap::new();
    for (p, a) in func.params.iter().zip(args.iter()) {
        env.insert(p.clone(), *a);
    }
    match exec_block(&func.body, &mut env, funcs)? {
        Some(v) => Ok(v),
        None => Err(format!("function `{}` did not return a value", func.name)),
    }
}

/// Executes a block; `Some(v)` means a `return` fired and `v` is its value.
fn exec_block(
    stmts: &[Stmt],
    env: &mut HashMap<String, u8>,
    funcs: &Funcs,
) -> Result<Option<u8>, String> {
    for stmt in stmts {
        if let Some(v) = exec_stmt(stmt, env, funcs)? {
            return Ok(Some(v));
        }
    }
    Ok(None)
}

fn exec_stmt(
    stmt: &Stmt,
    env: &mut HashMap<String, u8>,
    funcs: &Funcs,
) -> Result<Option<u8>, String> {
    match stmt {
        Stmt::Let { name, value } => {
            let v = eval(value, env, funcs)?;
            env.insert(name.clone(), v);
            Ok(None)
        }
        Stmt::Assign { name, value } => {
            if !env.contains_key(name) {
                return Err(format!("assignment to undeclared variable `{name}`"));
            }
            let v = eval(value, env, funcs)?;
            env.insert(name.clone(), v);
            Ok(None)
        }
        Stmt::Return(expr) => Ok(Some(eval(expr, env, funcs)?)),
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            if eval(cond, env, funcs)? != 0 {
                exec_block(then_branch, env, funcs)
            } else if let Some(eb) = else_branch {
                exec_block(eb, env, funcs)
            } else {
                Ok(None)
            }
        }
        Stmt::While { cond, body } => {
            while eval(cond, env, funcs)? != 0 {
                if let Some(v) = exec_block(body, env, funcs)? {
                    return Ok(Some(v));
                }
            }
            Ok(None)
        }
    }
}

fn eval(expr: &Expr, env: &HashMap<String, u8>, funcs: &Funcs) -> Result<u8, String> {
    match expr {
        Expr::Int(n) => Ok(*n),
        Expr::Var(name) => env
            .get(name)
            .copied()
            .ok_or_else(|| format!("undefined variable `{name}`")),
        Expr::Call { name, args } => {
            let f = funcs
                .get(name.as_str())
                .ok_or_else(|| format!("undefined function `{name}`"))?;
            let mut argv = Vec::with_capacity(args.len());
            for a in args {
                argv.push(eval(a, env, funcs)?);
            }
            call(f, &argv, funcs)
        }
        Expr::BinOp { op, left, right } => {
            let l = eval(left, env, funcs)?;
            let r = eval(right, env, funcs)?;
            Ok(match op {
                BinOp::Add => l.checked_add(r).ok_or("u8 overflow in `+`")?,
                BinOp::Sub => l.checked_sub(r).ok_or("u8 underflow in `-`")?,
                BinOp::Mul => l.checked_mul(r).ok_or("u8 overflow in `*`")?,
                BinOp::Div => l.checked_div(r).ok_or("division by zero")?,
                BinOp::Mod => l.checked_rem(r).ok_or("remainder by zero")?,
                BinOp::Lt => (l < r) as u8,
                BinOp::Gt => (l > r) as u8,
                BinOp::Le => (l <= r) as u8,
                BinOp::Ge => (l >= r) as u8,
                BinOp::Eq => (l == r) as u8,
                BinOp::Ne => (l != r) as u8,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn eval_src(src: &str) -> u8 {
        let toks = Lexer::new(src).tokenize().unwrap();
        let prog = Parser::new(toks).parse_program().unwrap();
        run(&prog).unwrap()
    }

    #[test]
    fn fib_recursion() {
        let src = "fn fib(n: u8) u8 { if (n < 2) { return n; } return fib(n - 1) + fib(n - 2); } pub fn main() u8 { return fib(10); }";
        assert_eq!(eval_src(src), 55);
    }

    #[test]
    fn while_loop_sum() {
        let src = "pub fn main() u8 { var i: u8 = 0; var s: u8 = 0; while (i < 5) { i = i + 1; s = s + i; } return s; }";
        assert_eq!(eval_src(src), 15);
    }
}
