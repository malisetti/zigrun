// C backend for zigrun: lowers the Zig-subset AST to C source. zigrun is a real
// compiler — it emits C, which `cc` compiles to a native executable; the
// program's `main() u8` becomes the process exit code. (Previously zigrun
// tree-walked the AST; now it generates code.)
//
// u8 semantics here are C's `uint8_t` (wrapping), a known divergence from Zig's
// checked arithmetic — tracked in FEATURES.md.

use crate::ast::{BinOp, Expr, Function, IntType, Program, Stmt, Type};
use std::collections::HashMap;
use std::fmt::Write;

pub fn emit_c(program: &Program) -> Result<String, String> {
    if !program.functions.iter().any(|f| f.name == "main") {
        return Err("no `main` function".to_string());
    }
    let mut out = String::new();
    out.push_str("#include <stdint.h>\n#include <stdbool.h>\n\n");

    for f in &program.functions {
        let _ = writeln!(out, "{};", prototype(f));
    }
    out.push('\n');

    for f in &program.functions {
        emit_function(&mut out, f)?;
        out.push('\n');
    }

    out.push_str("int main(void) {\n    return (int)zig_main();\n}\n");
    Ok(out)
}

fn c_fn(name: &str) -> String {
    format!("zig_{name}")
}

fn c_type(ty: Type) -> &'static str {
    match ty {
        Type::Bool => "bool",
        Type::Int(IntType::U8) => "uint8_t",
        Type::Int(IntType::U16) => "uint16_t",
        Type::Int(IntType::U32) => "uint32_t",
        Type::Int(IntType::U64) => "uint64_t",
        Type::Int(IntType::I8) => "int8_t",
        Type::Int(IntType::I16) => "int16_t",
        Type::Int(IntType::I32) => "int32_t",
        Type::Int(IntType::I64) => "int64_t",
    }
}

fn c_int_type(ty: IntType) -> &'static str {
    c_type(Type::Int(ty))
}

fn prototype(f: &Function) -> String {
    let params = if f.params.is_empty() {
        "void".to_string()
    } else {
        f.params
            .iter()
            .map(|(p, ty)| format!("{} {p}", c_type(*ty)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "{} {}({})",
        c_type(f.return_type),
        c_fn(&f.name),
        params
    )
}

fn emit_function(out: &mut String, f: &Function) -> Result<(), String> {
    let _ = writeln!(out, "{} {{", prototype(f));
    let mut env: HashMap<String, Type> = HashMap::new();
    for (name, ty) in &f.params {
        env.insert(name.clone(), *ty);
    }
    for s in &f.body {
        emit_stmt(out, s, 1, &mut env, f.return_type)?;
    }
    out.push_str("}\n");
    Ok(())
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn expr_type(expr: &Expr, env: &HashMap<String, Type>) -> Type {
    match expr {
        Expr::Int(_) => Type::Int(IntType::U8),
        Expr::Bool(_) => Type::Bool,
        Expr::Var(name) => env.get(name).copied().unwrap_or(Type::Int(IntType::U8)),
        Expr::BinOp { op, left, right } => match op {
            BinOp::LogicalAnd | BinOp::LogicalOr => Type::Bool,
            _ => combine_types(expr_type(left, env), expr_type(right, env)),
        },
        Expr::Call { .. } => Type::Int(IntType::U8),
        Expr::Switch { default, .. } => expr_type(default, env),
        Expr::IntCast { target, .. } => Type::Int(*target),
        Expr::UnaryNeg(inner) => expr_type(inner, env),
        Expr::UnaryNot(_) => Type::Bool,
    }
}

fn combine_types(a: Type, b: Type) -> Type {
    match (a, b) {
        (Type::Int(x), Type::Int(y)) => Type::Int(wider_int_type(x, y)),
        (Type::Int(x), _) => Type::Int(x),
        (_, Type::Int(y)) => Type::Int(y),
        _ => Type::Bool,
    }
}

fn wider_int_type(a: IntType, b: IntType) -> IntType {
    let ra = a.rank();
    let rb = b.rank();
    if ra > rb {
        a
    } else if rb > ra {
        b
    } else if a.is_signed() {
        a
    } else if b.is_signed() {
        b
    } else {
        a
    }
}

fn emit_stmt(
    out: &mut String,
    stmt: &Stmt,
    depth: usize,
    env: &mut HashMap<String, Type>,
    return_type: Type,
) -> Result<(), String> {
    indent(out, depth);
    match stmt {
        Stmt::Let { name, ty, value } => {
            let _ = writeln!(
                out,
                "{} {name} = {};",
                c_type(*ty),
                emit_expr(value, env, Some(*ty))?
            );
            env.insert(name.clone(), *ty);
        }
        Stmt::Assign { name, value } => {
            let ty = env.get(name).copied().unwrap_or(Type::Int(IntType::U8));
            let _ = writeln!(out, "{name} = {};", emit_expr(value, env, Some(ty))?);
        }
        Stmt::Return(e) => {
            let _ = writeln!(
                out,
                "return {};",
                emit_expr(e, env, Some(return_type))?
            );
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let _ = writeln!(out, "if ({}) {{", emit_expr(cond, env, None)?);
            for s in then_branch {
                emit_stmt(out, s, depth + 1, env, return_type)?;
            }
            indent(out, depth);
            out.push('}');
            if let Some(eb) = else_branch {
                out.push_str(" else {\n");
                for s in eb {
                    emit_stmt(out, s, depth + 1, env, return_type)?;
                }
                indent(out, depth);
                out.push('}');
            }
            out.push('\n');
        }
        Stmt::While { cond, body } => {
            let _ = writeln!(out, "while ({}) {{", emit_expr(cond, env, None)?);
            for s in body {
                emit_stmt(out, s, depth + 1, env, return_type)?;
            }
            indent(out, depth);
            out.push_str("}\n");
        }
        Stmt::Break => {
            let _ = writeln!(out, "break;");
        }
        Stmt::Continue => {
            let _ = writeln!(out, "continue;");
        }
        Stmt::For {
            capture,
            start,
            end,
            body,
        } => {
            let var = capture.as_deref().unwrap_or("_zig_for_i");
            let loop_ty = combine_types(expr_type(start, env), expr_type(end, env))
                .int_type()
                .unwrap_or(IntType::U8);
            let _ = writeln!(
                out,
                "for ({} {var} = {}; {var} < {}; {var}++) {{",
                c_int_type(loop_ty),
                emit_expr(start, env, Some(Type::Int(loop_ty)))?,
                emit_expr(end, env, Some(Type::Int(loop_ty)))?
            );
            if let Some(cap) = capture {
                env.insert(cap.clone(), Type::Int(loop_ty));
            }
            for s in body {
                emit_stmt(out, s, depth + 1, env, return_type)?;
            }
            if let Some(cap) = capture {
                env.remove(cap);
            }
            indent(out, depth);
            out.push_str("}\n");
        }
    }
    Ok(())
}

fn emit_expr(
    expr: &Expr,
    env: &HashMap<String, Type>,
    expected: Option<Type>,
) -> Result<String, String> {
    Ok(match expr {
        Expr::Int(n) => {
            if let Some(ty) = expected {
                format!("({})({})", c_type(ty), n)
            } else {
                n.to_string()
            }
        }
        Expr::Bool(v) => {
            if *v {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Expr::Var(name) => name.clone(),
        Expr::Call { name, args } => {
            let mut parts = Vec::with_capacity(args.len());
            for a in args {
                parts.push(emit_expr(a, env, None)?);
            }
            format!("{}({})", c_fn(name), parts.join(", "))
        }
        Expr::BinOp { op, left, right } => {
            if matches!(op, BinOp::LogicalAnd | BinOp::LogicalOr) {
                format!(
                    "({} {} {})",
                    emit_expr(left, env, Some(Type::Bool))?,
                    c_op(*op),
                    emit_expr(right, env, Some(Type::Bool))?
                )
            } else {
                let ty = combine_types(expr_type(left, env), expr_type(right, env));
                let expected_ty = expected.unwrap_or(ty);
                format!(
                    "({} {} {})",
                    emit_expr(left, env, Some(expected_ty))?,
                    c_op(*op),
                    emit_expr(right, env, Some(expected_ty))?
                )
            }
        }
        Expr::Switch {
            scrutinee,
            arms,
            default,
        } => emit_switch(scrutinee, arms, default, env)?,
        Expr::IntCast { expr, target } => {
            format!(
                "({})({})",
                c_int_type(*target),
                emit_expr(expr, env, None)?
            )
        }
        Expr::UnaryNeg(operand) => {
            let ty = expected.unwrap_or(expr_type(operand, env));
            format!(
                "(-({}))",
                emit_expr(operand, env, Some(ty))?
            )
        }
        Expr::UnaryNot(operand) => {
            format!(
                "(!({}))",
                emit_expr(operand, env, Some(Type::Bool))?
            )
        }
    })
}

fn emit_switch(
    scrutinee: &Expr,
    arms: &[(u64, Expr)],
    default: &Expr,
    env: &HashMap<String, Type>,
) -> Result<String, String> {
    let s = emit_expr(scrutinee, env, None)?;
    let mut result = emit_expr(default, env, None)?;
    for (val, arm_expr) in arms.iter().rev() {
        let arm = emit_expr(arm_expr, env, None)?;
        result = format!("(({s}) == {val} ? ({arm}) : ({result}))");
    }
    Ok(result)
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
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::LogicalAnd => "&&",
        BinOp::LogicalOr => "||",
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
        assert!(c.contains("zig_fib((n -"));
        assert!(c.contains("int main(void)"));
        assert!(c.contains("return (int)zig_main();"));
    }
}
