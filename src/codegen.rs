// C backend for zigrun: lowers the Zig-subset AST to C source. zigrun is a real
// compiler — it emits C, which `cc` compiles to a native executable; the
// program's `main() u8` becomes the process exit code. (Previously zigrun
// tree-walked the AST; now it generates code.)
//
// u8 semantics here are C's `uint8_t` (wrapping), a known divergence from Zig's
// checked arithmetic — tracked in FEATURES.md.

use crate::ast::{
    AssignTarget, BinOp, Expr, Function, IntType, Program, Stmt, StructDecl, SwitchCase, Type,
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
        let _ = writeln!(out, "typedef enum {{");
        for v in &e.variants {
            let _ = writeln!(out, "    {v},");
        }
        let _ = writeln!(out, "}} {};", e.name);
        out.push('\n');
    }

    for s in &program.structs {
        emit_struct_def(&mut out, s)?;
    }

    for inner in collect_optional_inners(program) {
        emit_optional_typedef(&mut out, &inner)?;
    }

    for f in &program.functions {
        let _ = writeln!(out, "{};", prototype(f));
    }
    out.push('\n');

    let layouts: HashMap<String, Vec<(String, Type)>> = program
        .structs
        .iter()
        .map(|s| (s.name.clone(), s.fields.clone()))
        .collect();

    for f in &program.functions {
        emit_function(&mut out, f, &layouts)?;
        out.push('\n');
    }

    out.push_str("int main(void) {\n    return (int)zig_main();\n}\n");
    Ok(out)
}

fn c_fn(name: &str) -> String {
    format!("zig_{name}")
}

fn emit_struct_def(out: &mut String, s: &StructDecl) -> Result<(), String> {
    let _ = writeln!(out, "typedef struct {{");
    for (name, ty) in &s.fields {
        let _ = writeln!(out, "    {} {};", c_type(ty), name);
    }
    let _ = writeln!(out, "}} {};", s.name);
    out.push('\n');
    Ok(())
}

fn opt_c_name(inner: &Type) -> String {
    format!("zigrun_opt_{}", c_type(inner).replace(' ', "_"))
}

fn emit_optional_typedef(out: &mut String, inner: &Type) -> Result<(), String> {
    let name = opt_c_name(inner);
    let _ = writeln!(
        out,
        "typedef struct {{ {} value; bool present; }} {};",
        c_type(inner),
        name
    );
    out.push('\n');
    Ok(())
}

fn collect_optional_inners(program: &Program) -> Vec<Type> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    let mut add = |ty: &Type| {
        if let Type::Optional { inner } = ty {
            let key = format!("{:?}", inner);
            if seen.insert(key) {
                result.push(inner.as_ref().clone());
            }
        }
    };
    for f in &program.functions {
        add(&f.return_type);
        for (_, ty) in &f.params {
            add(ty);
        }
        for s in &f.body {
            collect_optional_in_stmt(s, &mut add);
        }
    }
    result
}

fn collect_optional_in_stmt(stmt: &Stmt, add: &mut dyn FnMut(&Type)) {
    match stmt {
        Stmt::Let { ty, value, .. } => {
            add(ty);
            collect_optional_in_expr(value, add);
        }
        Stmt::Assign { value, .. } => collect_optional_in_expr(value, add),
        Stmt::Return(e) => collect_optional_in_expr(e, add),
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            collect_optional_in_expr(cond, add);
            for s in then_branch {
                collect_optional_in_stmt(s, add);
            }
            if let Some(eb) = else_branch {
                for s in eb {
                    collect_optional_in_stmt(s, add);
                }
            }
        }
        Stmt::While {
            cond,
            body,
            continue_stmt,
        } => {
            collect_optional_in_expr(cond, add);
            for s in body {
                collect_optional_in_stmt(s, add);
            }
            if let Some(cs) = continue_stmt {
                collect_optional_in_stmt(cs, add);
            }
        }
        Stmt::ForRange { start, end, body, .. } => {
            collect_optional_in_expr(start, add);
            collect_optional_in_expr(end, add);
            for s in body {
                collect_optional_in_stmt(s, add);
            }
        }
        Stmt::ForArray { body, .. } => {
            for s in body {
                collect_optional_in_stmt(s, add);
            }
        }
        Stmt::Break | Stmt::Continue => {}
    }
}

fn collect_optional_in_expr(expr: &Expr, add: &mut dyn FnMut(&Type)) {
    match expr {
        Expr::Orelse { opt, default } => {
            collect_optional_in_expr(opt, add);
            collect_optional_in_expr(default, add);
        }
        Expr::BinOp { left, right, .. } => {
            collect_optional_in_expr(left, add);
            collect_optional_in_expr(right, add);
        }
        Expr::Call { args, .. } => {
            for a in args {
                collect_optional_in_expr(a, add);
            }
        }
        Expr::Switch {
            scrutinee,
            arms,
            default,
        } => {
            collect_optional_in_expr(scrutinee, add);
            for (_, e) in arms {
                collect_optional_in_expr(e, add);
            }
            if let Some(d) = default {
                collect_optional_in_expr(d, add);
            }
        }
        Expr::IntCast { expr, .. } | Expr::UnaryNeg(expr) | Expr::UnaryNot(expr) => {
            collect_optional_in_expr(expr, add);
        }
        Expr::Mod { left, right } | Expr::Rem { left, right } => {
            collect_optional_in_expr(left, add);
            collect_optional_in_expr(right, add);
        }
        Expr::ArrayLiteral { elems, .. } => {
            for e in elems {
                collect_optional_in_expr(e, add);
            }
        }
        Expr::Index { base, index } => {
            collect_optional_in_expr(base, add);
            collect_optional_in_expr(index, add);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                collect_optional_in_expr(e, add);
            }
        }
        Expr::FieldAccess { base, .. } => collect_optional_in_expr(base, add),
        _ => {}
    }
}

fn c_type(ty: &Type) -> String {
    match ty {
        Type::Bool => "bool".to_string(),
        Type::Optional { inner } => opt_c_name(inner),
        Type::Int(IntType::U8) => "uint8_t".to_string(),
        Type::Int(IntType::U16) => "uint16_t".to_string(),
        Type::Int(IntType::U32) => "uint32_t".to_string(),
        Type::Int(IntType::U64) => "uint64_t".to_string(),
        Type::Int(IntType::I8) => "int8_t".to_string(),
        Type::Int(IntType::I16) => "int16_t".to_string(),
        Type::Int(IntType::I32) => "int32_t".to_string(),
        Type::Int(IntType::I64) => "int64_t".to_string(),
        Type::Array { .. } => ty
            .scalar_int_type()
            .map(|t| c_type(&Type::Int(t)))
            .unwrap_or_else(|| "uint8_t".to_string()),
        Type::Enum(name) => name.clone(),
        Type::Struct(name) => name.clone(),
    }
}

fn array_decl_parts(ty: &Type) -> (String, String) {
    match ty {
        Type::Array { len, elem } => {
            let (base, mut dims) = array_decl_parts(elem);
            dims.push_str(&format!("[{len}]"));
            (base, dims)
        }
        other => (c_type(other), String::new()),
    }
}

fn c_var_decl(name: &str, ty: &Type) -> String {
    match ty {
        Type::Array { .. } => {
            let (base, dims) = array_decl_parts(ty);
            format!("{base} {name}{dims}")
        }
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

fn emit_function(
    out: &mut String,
    f: &Function,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Result<(), String> {
    let _ = writeln!(out, "{} {{", prototype(f));
    let mut env: HashMap<String, Type> = HashMap::new();
    for (name, ty) in &f.params {
        env.insert(name.clone(), ty.clone());
    }
    for s in &f.body {
        emit_stmt(out, s, 1, &mut env, &f.return_type, layouts)?;
    }
    out.push_str("}\n");
    Ok(())
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn expr_type(
    expr: &Expr,
    env: &HashMap<String, Type>,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Type {
    match expr {
        Expr::Int(_) => Type::Int(IntType::U8),
        Expr::Bool(_) => Type::Bool,
        Expr::Var(name) => env
            .get(name)
            .cloned()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::BinOp { op, left, right } => match op {
            BinOp::LogicalAnd | BinOp::LogicalOr => Type::Bool,
            _ => combine_types(expr_type(left, env, layouts), expr_type(right, env, layouts)),
        },
        Expr::Call { .. } => Type::Int(IntType::U8),
        Expr::Switch { default, .. } => default
            .as_ref()
            .map(|d| expr_type(d, env, layouts))
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::EnumLiteral { enum_name, .. } => {
            if enum_name.is_empty() {
                Type::Int(IntType::U8)
            } else {
                Type::Enum(enum_name.clone())
            }
        }
        Expr::IntCast { target, .. } => Type::Int(*target),
        Expr::Mod { left, right } | Expr::Rem { left, right } => {
            combine_types(expr_type(left, env, layouts), expr_type(right, env, layouts))
        }
        Expr::UnaryNeg(inner) => expr_type(inner, env, layouts),
        Expr::UnaryNot(_) => Type::Bool,
        Expr::ArrayLiteral { elems, annotated } => {
            if let Some((len, elem)) = annotated {
                Type::Array {
                    len: *len,
                    elem: Box::new(Type::Int(*elem)),
                }
            } else if let Some(first) = elems.first() {
                let elem = expr_type(first, env, layouts)
                    .scalar_int_type()
                    .unwrap_or(IntType::U8);
                Type::Array {
                    len: elems.len(),
                    elem: Box::new(Type::Int(elem)),
                }
            } else {
                Type::Array {
                    len: 0,
                    elem: Box::new(Type::Int(IntType::U8)),
                }
            }
        }
        Expr::Index { base, .. } => index_expr_type(base, env, layouts),
        Expr::Undefined => Type::Int(IntType::U8),
        Expr::StructLiteral { struct_name, .. } => Type::Struct(struct_name.clone()),
        Expr::FieldAccess { base, field } => field_type(base, field, env, layouts),
        Expr::Null => Type::Optional {
            inner: Box::new(Type::Int(IntType::U8)),
        },
        Expr::Orelse { default, .. } => expr_type(default, env, layouts),
    }
}

fn index_expr_type(
    base: &Expr,
    env: &HashMap<String, Type>,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Type {
    let base_ty = match base {
        Expr::Var(name) => env.get(name).cloned(),
        other => Some(expr_type(other, env, layouts)),
    };
    base_ty
        .and_then(|t| t.array_elem())
        .unwrap_or(Type::Int(IntType::U8))
}

fn lookup_field(
    struct_name: &str,
    field: &str,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Option<Type> {
    layouts
        .get(struct_name)?
        .iter()
        .find(|(n, _)| n == field)
        .map(|(_, ty)| ty.clone())
}

fn field_type(
    base: &Expr,
    field: &str,
    env: &HashMap<String, Type>,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Type {
    let struct_name = match base {
        Expr::Var(name) => env.get(name).and_then(|t| t.struct_name().map(str::to_string)),
        Expr::StructLiteral { struct_name, .. } => Some(struct_name.clone()),
        Expr::FieldAccess { base, field: parent } => field_type(base, parent, env, layouts)
            .struct_name()
            .map(str::to_string),
        _ => None,
    };
    struct_name
        .and_then(|sn| lookup_field(&sn, field, layouts))
        .unwrap_or(Type::Int(IntType::U8))
}

fn combine_types(a: Type, b: Type) -> Type {
    match (a, b) {
        (Type::Int(x), Type::Int(y)) => Type::Int(wider_int_type(x, y)),
        (Type::Int(x), _) => Type::Int(x),
        (_, Type::Int(y)) => Type::Int(y),
        (Type::Array { elem, .. }, Type::Array { elem: elem2, .. }) => Type::Array {
            len: 0,
            elem: Box::new(combine_types(
                elem.as_ref().clone(),
                elem2.as_ref().clone(),
            )),
        },
        (Type::Array { elem, .. }, _) | (_, Type::Array { elem, .. }) => elem
            .scalar_int_type()
            .map(Type::Int)
            .unwrap_or(Type::Int(IntType::U8)),
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
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Result<(), String> {
    indent(out, depth);
    match stmt {
        Stmt::Let { name, ty, value } => {
            if matches!(value, Expr::Undefined) {
                let _ = writeln!(out, "{};", c_var_decl(name, ty));
            } else {
                let _ = writeln!(
                    out,
                    "{} = {};",
                    c_var_decl(name, ty),
                    emit_expr_with_optional_wrap(value, env, ty, layouts)?
                );
            }
            env.insert(name.clone(), ty.clone());
        }
        Stmt::Assign { target, value } => {
            let ty = assign_target_type(target, env, layouts);
            let lhs = emit_assign_target(target, env, layouts)?;
            let _ = writeln!(
                out,
                "{lhs} = {};",
                emit_expr(value, env, Some(ty), layouts)?
            );
        }
        Stmt::Return(e) => {
            let _ = writeln!(
                out,
                "return {};",
                emit_expr(e, env, Some(return_type.clone()), layouts)?
            );
        }
        Stmt::If {
            cond,
            then_branch,
            else_branch,
        } => {
            let _ = writeln!(out, "if ({}) {{", emit_expr(cond, env, None, layouts)?);
            for s in then_branch {
                emit_stmt(out, s, depth + 1, env, return_type, layouts)?;
            }
            indent(out, depth);
            out.push('}');
            if let Some(eb) = else_branch {
                out.push_str(" else {\n");
                for s in eb {
                    emit_stmt(out, s, depth + 1, env, return_type, layouts)?;
                }
                indent(out, depth);
                out.push('}');
            }
            out.push('\n');
        }
        Stmt::While {
            cond,
            body,
            continue_stmt,
        } => {
            let _ = writeln!(out, "while ({}) {{", emit_expr(cond, env, None, layouts)?);
            for s in body {
                emit_stmt(out, s, depth + 1, env, return_type, layouts)?;
            }
            if let Some(cs) = continue_stmt {
                emit_stmt(out, cs, depth + 1, env, return_type, layouts)?;
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
            let loop_ty = combine_types(expr_type(start, env, layouts), expr_type(end, env, layouts))
                .int_type()
                .unwrap_or(IntType::U8);
            let _ = writeln!(
                out,
                "for ({} {var} = {}; {var} < {}; {var}++) {{",
                c_int_type(loop_ty),
                emit_expr(start, env, Some(Type::Int(loop_ty)), layouts)?,
                emit_expr(end, env, Some(Type::Int(loop_ty)), layouts)?
            );
            if let Some(cap) = capture {
                env.insert(cap.clone(), Type::Int(loop_ty));
            }
            for s in body {
                emit_stmt(out, s, depth + 1, env, return_type, layouts)?;
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
                Type::Array { len, ref elem } => (
                    len,
                    elem.scalar_int_type().ok_or_else(|| {
                        format!("for-loop expected array of integers, found {arr_ty:?}")
                    })?,
                ),
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
                emit_stmt(out, s, depth + 1, env, return_type, layouts)?;
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

fn assign_target_type(
    target: &AssignTarget,
    env: &HashMap<String, Type>,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Type {
    match target {
        AssignTarget::Name(name) => env
            .get(name)
            .cloned()
            .unwrap_or(Type::Int(IntType::U8)),
        AssignTarget::Index { base, .. } => index_expr_type(base, env, layouts),
    }
}

fn emit_assign_target(
    target: &AssignTarget,
    env: &HashMap<String, Type>,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Result<String, String> {
    Ok(match target {
        AssignTarget::Name(name) => name.clone(),
        AssignTarget::Index { base, index } => format!(
            "{}[{}]",
            emit_expr(base, env, None, layouts)?,
            emit_expr(index, env, None, layouts)?
        ),
    })
}

fn emit_expr_with_optional_wrap(
    expr: &Expr,
    env: &HashMap<String, Type>,
    ty: &Type,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Result<String, String> {
    if let Type::Optional { inner } = ty {
        if matches!(expr, Expr::Int(_) | Expr::Bool(_) | Expr::Var(_)) {
            let inner_ty = inner.as_ref().clone();
            let val = emit_expr(expr, env, Some(inner_ty.clone()), layouts)?;
            let opt_name = opt_c_name(inner);
            return Ok(format!("({opt_name}){{ .value = {val}, .present = true }}"));
        }
        if matches!(expr, Expr::Null) {
            let opt_name = opt_c_name(inner);
            return Ok(format!(
                "({opt_name}){{ .value = ({})0, .present = false }}",
                c_type(inner)
            ));
        }
    }
    emit_expr(expr, env, Some(ty.clone()), layouts)
}

fn emit_expr(
    expr: &Expr,
    env: &HashMap<String, Type>,
    expected: Option<Type>,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Result<String, String> {
    Ok(match expr {
        Expr::Int(n) => {
            if let Some(ty) = &expected {
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
                parts.push(emit_expr(a, env, None, layouts)?);
            }
            format!("{}({})", c_fn(name), parts.join(", "))
        }
        Expr::BinOp { op, left, right } => {
            if matches!(op, BinOp::LogicalAnd | BinOp::LogicalOr) {
                format!(
                    "({} {} {})",
                    emit_expr(left, env, Some(Type::Bool), layouts)?,
                    c_op(*op),
                    emit_expr(right, env, Some(Type::Bool), layouts)?
                )
            } else {
                let ty = combine_types(expr_type(left, env, layouts), expr_type(right, env, layouts));
                let expected_ty = expected.unwrap_or(ty);
                format!(
                    "({} {} {})",
                    emit_expr(left, env, Some(expected_ty.clone()), layouts)?,
                    c_op(*op),
                    emit_expr(right, env, Some(expected_ty), layouts)?
                )
            }
        }
        Expr::Switch {
            scrutinee,
            arms,
            default,
        } => emit_switch(scrutinee, arms, default.as_deref(), env, expected, layouts)?,
        Expr::EnumLiteral { variant, .. } => variant.clone(),
        Expr::IntCast { expr, target } => {
            format!(
                "({})({})",
                c_int_type(*target),
                emit_expr(expr, env, None, layouts)?
            )
        }
        Expr::Mod { left, right } => {
            emit_mod_rem(left, right, env, expected, true, layouts)?
        }
        Expr::Rem { left, right } => {
            emit_mod_rem(left, right, env, expected, false, layouts)?
        }
        Expr::UnaryNeg(operand) => {
            let ty = expected.unwrap_or(expr_type(operand, env, layouts));
            format!(
                "(-({}))",
                emit_expr(operand, env, Some(ty), layouts)?
            )
        }
        Expr::UnaryNot(operand) => {
            format!(
                "(!({}))",
                emit_expr(operand, env, Some(Type::Bool), layouts)?
            )
        }
        Expr::ArrayLiteral { elems, annotated } => {
            let elem_ty = expected
                .and_then(|t| t.scalar_int_type())
                .or_else(|| annotated.map(|(_, elem)| elem))
                .or_else(|| {
                    elems
                        .first()
                        .map(|e| expr_type(e, env, layouts).int_type().unwrap_or(IntType::U8))
                })
                .unwrap_or(IntType::U8);
            let parts: Result<Vec<_>, _> = elems
                .iter()
                .map(|e| emit_expr(e, env, Some(Type::Int(elem_ty)), layouts))
                .collect();
            format!("{{ {} }}", parts?.join(", "))
        }
        Expr::Index { base, index } => format!(
            "{}[{}]",
            emit_expr(base, env, None, layouts)?,
            emit_expr(index, env, None, layouts)?
        ),
        Expr::Undefined => return Err("undefined may only appear in variable declarations".to_string()),
        Expr::StructLiteral { struct_name, fields } => {
            let mut parts = Vec::new();
            for (fname, val) in fields {
                let fty = lookup_field(struct_name, fname, layouts)
                    .unwrap_or(Type::Int(IntType::U8));
                parts.push(format!(
                    ".{fname} = {}",
                    emit_expr(val, env, Some(fty), layouts)?
                ));
            }
            format!("({struct_name}){{ {} }}", parts.join(", "))
        }
        Expr::FieldAccess { base, field } => {
            let base_c = emit_expr(base, env, None, layouts)?;
            format!("({base_c}).{field}")
        }
        Expr::Null => {
            let inner = expected
                .and_then(|t| t.optional_inner())
                .unwrap_or(Type::Int(IntType::U8));
            let opt_name = opt_c_name(&inner);
            format!(
                "({opt_name}){{ .value = ({})0, .present = false }}",
                c_type(&inner)
            )
        }
        Expr::Orelse { opt, default } => {
            let opt_ty = expr_type(opt, env, layouts);
            if opt_ty.optional_inner().is_none() {
                return Err(format!("orelse requires optional lhs, found {opt_ty:?}"));
            }
            let def_ty = expr_type(default, env, layouts);
            let opt_c = emit_expr(opt, env, Some(opt_ty), layouts)?;
            let def_c = emit_expr(default, env, Some(def_ty), layouts)?;
            format!(
                "(({opt_c}).present ? ({opt_c}).value : ({def_c}))"
            )
        }
    })
}

fn emit_mod_rem(
    left: &Expr,
    right: &Expr,
    env: &HashMap<String, Type>,
    expected: Option<Type>,
    is_mod: bool,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Result<String, String> {
    let ty = combine_types(expr_type(left, env, layouts), expr_type(right, env, layouts));
    let int_ty = expected
        .and_then(|t| t.int_type())
        .or_else(|| ty.int_type())
        .unwrap_or(IntType::U8);
    let ct = c_int_type(int_ty);
    let l = emit_expr(left, env, Some(Type::Int(int_ty)), layouts)?;
    let r = emit_expr(right, env, Some(Type::Int(int_ty)), layouts)?;
    if int_ty.is_signed() {
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
    arms: &[(SwitchCase, Expr)],
    default: Option<&Expr>,
    env: &HashMap<String, Type>,
    expected: Option<Type>,
    layouts: &HashMap<String, Vec<(String, Type)>>,
) -> Result<String, String> {
    let scrut_ty = expr_type(scrutinee, env, layouts);
    let s = emit_expr(scrutinee, env, Some(scrut_ty.clone()), layouts)?;
    let out_ty = expected.unwrap_or(Type::Int(IntType::U8));
    let mut result = match default {
        Some(d) => emit_expr(d, env, Some(out_ty.clone()), layouts)?,
        None => emit_expr(&Expr::Int(0), env, Some(out_ty.clone()), layouts)?,
    };
    for (case, arm_expr) in arms.iter().rev() {
        let arm = emit_expr(arm_expr, env, Some(out_ty.clone()), layouts)?;
        result = match case {
            SwitchCase::Int(val) => format!("(({s}) == {val} ? ({arm}) : ({result}))"),
            SwitchCase::Variant(v) => format!("(({s}) == {v} ? ({arm}) : ({result}))"),
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
