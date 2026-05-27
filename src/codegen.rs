// C backend for zigrun: lowers the Zig-subset AST to C source. zigrun is a real
// compiler — it emits C, which `cc` compiles to a native executable; the
// program's `main() u8` becomes the process exit code. (Previously zigrun
// tree-walked the AST; now it generates code.)
//
// u8 semantics here are C's `uint8_t` (wrapping), a known divergence from Zig's
// checked arithmetic — tracked in FEATURES.md.

use crate::ast::{
    AssignTarget, BinOp, EnumDef, Expr, Function, IntType, Program, Stmt, StructDef, SwitchArm,
    SwitchTag, Type,
};
use std::collections::HashMap;
use std::fmt::Write;

pub fn emit_c(program: &Program) -> Result<String, String> {
    if !program.functions.iter().any(|f| f.name == "main") {
        return Err("no `main` function".to_string());
    }
    let mut out = String::new();
    out.push_str("#include <stdint.h>\n#include <stdbool.h>\n#include <stddef.h>\n\n");

    for e in &program.enums {
        emit_enum_def(&mut out, e)?;
        out.push('\n');
    }

    for s in &program.structs {
        emit_struct_def(&mut out, s)?;
        out.push('\n');
    }

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

fn emit_struct_def(out: &mut String, s: &StructDef) -> Result<(), String> {
    let _ = writeln!(out, "typedef struct {{");
    for (field, ty) in &s.fields {
        let _ = writeln!(out, "    {} {};", c_type(ty), field);
    }
    let _ = writeln!(out, "}} {};", s.name);
    Ok(())
}

fn emit_enum_def(out: &mut String, e: &EnumDef) -> Result<(), String> {
    let _ = writeln!(out, "typedef enum {{");
    for v in &e.variants {
        let _ = writeln!(out, "    {}_{v},", e.name);
    }
    let _ = writeln!(out, "}} {};", e.name);
    Ok(())
}

fn c_enum_variant(enum_name: &str, variant: &str) -> String {
    format!("{enum_name}_{variant}")
}

fn c_type(ty: &Type) -> String {
    match ty {
        Type::Bool => "bool".to_string(),
        Type::Int(IntType::U8) => "uint8_t".to_string(),
        Type::Int(IntType::U16) => "uint16_t".to_string(),
        Type::Int(IntType::U32) => "uint32_t".to_string(),
        Type::Int(IntType::U64) => "uint64_t".to_string(),
        Type::Int(IntType::I8) => "int8_t".to_string(),
        Type::Int(IntType::I16) => "int16_t".to_string(),
        Type::Int(IntType::I32) => "int32_t".to_string(),
        Type::Int(IntType::I64) => "int64_t".to_string(),
        Type::Array { .. } => "uint8_t".to_string(),
        Type::Enum(name) => name.clone(),
        Type::Struct(name) => name.clone(),
    }
}

fn c_var_decl(name: &str, ty: &Type) -> String {
    match ty {
        Type::Array { len, elem } => format!("{} {}[{}]", c_int_type(*elem), name, len),
        Type::Struct(_) => format!("{} {name}", c_type(ty)),
        other => format!("{} {name}", c_type(other)),
    }
}

fn c_int_type(ty: IntType) -> String {
    c_type(&Type::Int(ty))
}

fn prototype(f: &Function) -> String {
    let params = if f.params.is_empty() {
        "void".to_string()
    } else {
        f.params
            .iter()
            .map(|(p, ty)| format!("{} {p}", c_type(ty)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "{} {}({})",
        c_type(&f.return_type),
        c_fn(&f.name),
        params
    )
}

fn emit_function(out: &mut String, f: &Function) -> Result<(), String> {
    let _ = writeln!(out, "{} {{", prototype(f));
    let mut env: HashMap<String, Type> = HashMap::new();
    for (name, ty) in &f.params {
        env.insert(name.clone(), ty.clone());
    }
    for s in &f.body {
        emit_stmt(out, s, 1, &mut env, &f.return_type)?;
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
        Expr::Var(name) => env
            .get(name)
            .cloned()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::EnumLiteral { enum_name, variant: _ } => Type::Enum(enum_name.clone()),
        Expr::BinOp { op, left, right } => match op {
            BinOp::LogicalAnd | BinOp::LogicalOr => Type::Bool,
            _ => combine_types(expr_type(left, env), expr_type(right, env)),
        },
        Expr::Call { .. } => Type::Int(IntType::U8),
        Expr::Switch { default, arms, .. } => default
            .as_ref()
            .map(|d| expr_type(d, env))
            .or_else(|| arms.last().map(|a| expr_type(&a.expr, env)))
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::IntCast { target, .. } => Type::Int(*target),
        Expr::Mod { left, right } | Expr::Rem { left, right } => {
            combine_types(expr_type(left, env), expr_type(right, env))
        }
        Expr::UnaryNeg(inner) => expr_type(inner, env),
        Expr::UnaryNot(_) => Type::Bool,
        Expr::ArrayLiteral { elems, annotated } => {
            if let Some((len, elem)) = annotated {
                Type::Array { len: *len, elem: *elem }
            } else if let Some(first) = elems.first() {
                let elem = expr_type(first, env)
                    .int_type()
                    .unwrap_or(IntType::U8);
                Type::Array {
                    len: elems.len(),
                    elem,
                }
            } else {
                Type::Array {
                    len: 0,
                    elem: IntType::U8,
                }
            }
        }
        Expr::Index { base, .. } => {
            env.get(base)
                .and_then(|t| t.array_elem().map(|elem| Type::Int(elem)))
                .unwrap_or(Type::Int(IntType::U8))
        }
        Expr::StructLiteral { struct_name, .. } => Type::Struct(struct_name.clone()),
        Expr::FieldAccess { base, field } => field_expr_type(base, field, env),
    }
}

fn field_expr_type(base: &Expr, field: &str, env: &HashMap<String, Type>) -> Type {
    let base_ty = match base {
        Expr::Var(name) => env.get(name).cloned(),
        Expr::FieldAccess { base, field: parent } => Some(field_expr_type(base, parent, env)),
        Expr::StructLiteral { struct_name, .. } => Some(Type::Struct(struct_name.clone())),
        _ => None,
    };
    // Field types are resolved from struct layout at emit time; use u8 fallback for inference.
    let _ = (base_ty, field);
    Type::Int(IntType::U8)
}

fn combine_types(a: Type, b: Type) -> Type {
    match (a, b) {
        (Type::Int(x), Type::Int(y)) => Type::Int(wider_int_type(x, y)),
        (Type::Int(x), _) => Type::Int(x),
        (_, Type::Int(y)) => Type::Int(y),
        (Type::Array { elem, .. }, Type::Array { elem: elem2, .. }) => {
            Type::Array {
                len: 0,
                elem: wider_int_type(elem, elem2),
            }
        }
        (Type::Array { elem, .. }, _) | (_, Type::Array { elem, .. }) => Type::Int(elem),
        (Type::Enum(a), Type::Enum(b)) if a == b => Type::Enum(a),
        (Type::Enum(a), _) | (_, Type::Enum(a)) => Type::Enum(a),
        (Type::Struct(a), Type::Struct(b)) if a == b => Type::Struct(a),
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
    return_type: &Type,
) -> Result<(), String> {
    indent(out, depth);
    match stmt {
        Stmt::Let { name, ty, value } => {
            let _ = writeln!(
                out,
                "{} = {};",
                c_var_decl(name, ty),
                emit_expr(value, env, Some(ty))?
            );
            env.insert(name.clone(), ty.clone());
        }
        Stmt::Assign { target, value } => {
            let ty = assign_target_type(target, env);
            let lhs = emit_assign_target(target, env)?;
            let _ = writeln!(
                out,
                "{lhs} = {};",
                emit_expr(value, env, Some(&ty))?
            );
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
        Stmt::ForRange {
            capture,
            start,
            end,
            body,
        } => {
            let var = capture.as_deref().unwrap_or("_zig_for_i");
            let loop_ty = combine_types(expr_type(start, env), expr_type(end, env))
                .int_type()
                .unwrap_or(IntType::U8);
            let loop_ty_type = Type::Int(loop_ty);
            let _ = writeln!(
                out,
                "for ({} {var} = {}; {var} < {}; {var}++) {{",
                c_int_type(loop_ty),
                emit_expr(start, env, Some(&loop_ty_type))?,
                emit_expr(end, env, Some(&loop_ty_type))?
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
        Stmt::ForArray {
            capture,
            array,
            body,
        } => {
            let arr_ty = env
                .get(array)
                .cloned()
                .ok_or_else(|| format!("unknown array variable {array}"))?;
            let (len, elem) = match arr_ty {
                Type::Array { len, elem } => (len, elem),
                other => return Err(format!("for-loop expected array type, found {other:?}")),
            };
            let cap = capture.as_deref().unwrap_or("_zig_for_x");
            let idx = format!("_{array}_i");
            let _ = writeln!(out, "for (size_t {idx} = 0; {idx} < {len}; {idx}++) {{");
            let _ = writeln!(
                out,
                "    {} {cap} = {array}[{idx}];",
                c_int_type(elem)
            );
            if let Some(name) = capture {
                env.insert(name.clone(), Type::Int(elem));
            }
            for s in body {
                emit_stmt(out, s, depth + 1, env, return_type)?;
            }
            if let Some(name) = capture {
                env.remove(name);
            }
            indent(out, depth);
            out.push_str("}\n");
        }
    }
    Ok(())
}

fn assign_target_type(target: &AssignTarget, env: &HashMap<String, Type>) -> Type {
    match target {
        AssignTarget::Name(name) => env
            .get(name)
            .cloned()
            .unwrap_or(Type::Int(IntType::U8)),
        AssignTarget::Index { base, .. } => env
            .get(base)
            .and_then(|t| t.array_elem().map(|elem| Type::Int(elem)))
            .unwrap_or(Type::Int(IntType::U8)),
    }
}

fn emit_assign_target(
    target: &AssignTarget,
    env: &HashMap<String, Type>,
) -> Result<String, String> {
    Ok(match target {
        AssignTarget::Name(name) => name.clone(),
        AssignTarget::Index { base, index } => {
            format!("{base}[{}]", emit_expr(index, env, None)?)
        }
    })
}

fn emit_expr(
    expr: &Expr,
    env: &HashMap<String, Type>,
    expected: Option<&Type>,
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
        Expr::EnumLiteral { enum_name, variant } => c_enum_variant(enum_name, variant),
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
                    emit_expr(left, env, Some(&Type::Bool))?,
                    c_op(*op),
                    emit_expr(right, env, Some(&Type::Bool))?
                )
            } else {
                let ty = combine_types(expr_type(left, env), expr_type(right, env));
                let expected_ty = expected.cloned().unwrap_or(ty);
                format!(
                    "({} {} {})",
                    emit_expr(left, env, Some(&expected_ty))?,
                    c_op(*op),
                    emit_expr(right, env, Some(&expected_ty))?
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
        Expr::Mod { left, right } => {
            emit_mod_rem(left, right, env, expected, true)?
        }
        Expr::Rem { left, right } => {
            emit_mod_rem(left, right, env, expected, false)?
        }
        Expr::UnaryNeg(operand) => {
            let ty = expected.cloned().unwrap_or_else(|| expr_type(operand, env));
            format!(
                "(-({}))",
                emit_expr(operand, env, Some(&ty))?
            )
        }
        Expr::UnaryNot(operand) => {
            format!(
                "(!({}))",
                emit_expr(operand, env, Some(&Type::Bool))?
            )
        }
        Expr::ArrayLiteral { elems, annotated } => {
            let elem_ty = expected
                .and_then(|t| t.array_elem())
                .or_else(|| annotated.map(|(_, elem)| elem))
                .or_else(|| {
                    elems
                        .first()
                        .map(|e| expr_type(e, env).int_type().unwrap_or(IntType::U8))
                })
                .unwrap_or(IntType::U8);
            let parts: Result<Vec<_>, _> = elems
                .iter()
                .map(|e| emit_expr(e, env, Some(&Type::Int(elem_ty))))
                .collect();
            format!("{{ {} }}", parts?.join(", "))
        }
        Expr::Index { base, index } => {
            format!(
                "{base}[{}]",
                emit_expr(index, env, Some(&Type::Int(IntType::U32)))?
            )
        }
        Expr::StructLiteral { struct_name, fields } => {
            let mut parts = Vec::new();
            for (field, value) in fields {
                parts.push(format!(
                    ".{field} = {}",
                    emit_expr(value, env, None)?
                ));
            }
            format!("({struct_name}){{ {} }}", parts.join(", "))
        }
        Expr::FieldAccess { base, field } => {
            format!(
                "({}).{field}",
                emit_expr(base, env, None)?
            )
        }
    })
}

fn emit_mod_rem(
    left: &Expr,
    right: &Expr,
    env: &HashMap<String, Type>,
    expected: Option<&Type>,
    is_mod: bool,
) -> Result<String, String> {
    let ty = combine_types(expr_type(left, env), expr_type(right, env));
    let it = expected
        .and_then(|t| t.int_type())
        .or_else(|| ty.int_type())
        .unwrap_or(IntType::U8);
    let ct = c_int_type(it);
    let int_type = Type::Int(it);
    let l = emit_expr(left, env, Some(&int_type))?;
    let r = emit_expr(right, env, Some(&int_type))?;
    if it.is_signed() {
        if is_mod {
            Ok(format!(
                "(({ct} __a = ({l}), {ct} __b = ({r}), {ct} __m = __a % __b, \
                 (__m != 0 && ((__m < 0) != (__b < 0))) ? __m + __b : __m))"
            ))
        } else {
            Ok(format!("(({ct})({l}) % ({ct})({r}))"))
        }
    } else {
        Ok(format!("(({ct})({l}) % ({ct})({r}))"))
    }
}

fn emit_switch(
    scrutinee: &Expr,
    arms: &[SwitchArm],
    default: &Option<Box<Expr>>,
    env: &HashMap<String, Type>,
) -> Result<String, String> {
    if arms.is_empty() {
        return Err("switch has no arms".to_string());
    }
    let s = emit_expr(scrutinee, env, None)?;
    let scrutinee_ty = expr_type(scrutinee, env);
    let mut result = match default {
        Some(d) => emit_expr(d, env, None)?,
        None => emit_expr(&arms[arms.len() - 1].expr, env, None)?,
    };
    let arm_iter: Box<dyn Iterator<Item = &SwitchArm>> = match default {
        Some(_) => Box::new(arms.iter().rev()),
        None => Box::new(arms.iter().rev().skip(1)),
    };
    for arm in arm_iter {
        let arm_expr = emit_expr(&arm.expr, env, None)?;
        result = match (&scrutinee_ty, &arm.tag) {
            (Type::Enum(enum_name), SwitchTag::EnumVariant { variant, .. }) => {
                let tag = c_enum_variant(enum_name, variant);
                format!("(({s}) == {tag} ? ({arm_expr}) : ({result}))")
            }
            (_, SwitchTag::Int(val)) => {
                format!("(({s}) == {val} ? ({arm_expr}) : ({result}))")
            }
            _ => return Err("switch arm tag does not match scrutinee type".to_string()),
        };
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
