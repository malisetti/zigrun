// C backend for zigrun: lowers the Zig-subset AST to C source. zigrun is a real
// compiler — it emits C, which `cc` compiles to a native executable; the
// program's `main() u8` becomes the process exit code. (Previously zigrun
// tree-walked the AST; now it generates code.)
//
// u8 semantics here are C's `uint8_t` (wrapping), a known divergence from Zig's
// checked arithmetic — tracked in FEATURES.md.

use crate::ast::{
    ArrayLen, AssignTarget, BinOp, EnumDef, ErrorSetDef, Expr, Function, GlobalVar, IntType,
    Program, Stmt, StructDef, SwitchArm, SwitchTag, Type, UnionDef, UnionVariant,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

thread_local! {
    static ENUM_DEFS: RefCell<HashMap<String, EnumDef>> = RefCell::new(HashMap::new());
    static ERROR_SET_DEFS: RefCell<HashMap<String, ErrorSetDef>> = RefCell::new(HashMap::new());
    static UNION_DEFS: RefCell<HashMap<String, UnionDef>> = RefCell::new(HashMap::new());
    static STRUCT_DEFS: RefCell<HashMap<String, StructDef>> = RefCell::new(HashMap::new());
    static FUNC_PARAMS: RefCell<HashMap<String, Vec<Type>>> = RefCell::new(HashMap::new());
}

fn with_type_defs<R>(
    enums: &HashMap<String, EnumDef>,
    error_sets: &HashMap<String, ErrorSetDef>,
    unions: &HashMap<String, UnionDef>,
    structs: &HashMap<String, StructDef>,
    f: impl FnOnce() -> R,
) -> R {
    ENUM_DEFS.with(|cell| {
        *cell.borrow_mut() = enums.clone();
        ERROR_SET_DEFS.with(|e_cell| {
            *e_cell.borrow_mut() = error_sets.clone();
            UNION_DEFS.with(|u_cell| {
                *u_cell.borrow_mut() = unions.clone();
                STRUCT_DEFS.with(|s_cell| {
                    *s_cell.borrow_mut() = structs.clone();
                    f()
                })
            })
        })
    })
}

fn lookup_enum(name: &str) -> Option<EnumDef> {
    ENUM_DEFS.with(|cell| cell.borrow().get(name).cloned())
}

fn lookup_error_set(name: &str) -> Option<ErrorSetDef> {
    ERROR_SET_DEFS.with(|cell| cell.borrow().get(name).cloned())
}

fn lookup_union(name: &str) -> Option<UnionDef> {
    UNION_DEFS.with(|cell| cell.borrow().get(name).cloned())
}

fn lookup_struct(name: &str) -> Option<StructDef> {
    STRUCT_DEFS.with(|cell| cell.borrow().get(name).cloned())
}

fn lookup_func_params(name: &str) -> Option<Vec<Type>> {
    FUNC_PARAMS.with(|cell| cell.borrow().get(name).cloned())
}

fn emit_field_access(
    base: &Expr,
    field: &str,
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
    fn_return_type: &Type,
    temp_id: &mut usize,
) -> Result<String, String> {
    let base_ty = expr_type(base, env, func_returns);
    let base_s = emit_expr(base, env, None, func_returns, fn_return_type, temp_id)?;
    if base_ty.is_slice() {
        Ok(format!("({base_s}).{field}"))
    } else if base_ty.is_pointer() {
        Ok(format!("({base_s})->{field}"))
    } else {
        Ok(format!("({base_s}).{field}"))
    }
}

pub fn emit_c(program: &Program) -> Result<String, String> {
    if !program.functions.iter().any(|f| f.name == "main") {
        return Err("no `main` function".to_string());
    }
    let mut out = String::new();
    out.push_str(
        "#include <stdint.h>\n#include <stdbool.h>\n#include <stddef.h>\n#include <stdio.h>\n\n",
    );

    let enum_defs: HashMap<String, EnumDef> = program
        .enums
        .iter()
        .map(|e| (e.name.clone(), e.clone()))
        .collect();
    let error_set_defs: HashMap<String, ErrorSetDef> = program
        .error_sets
        .iter()
        .map(|e| (e.name.clone(), e.clone()))
        .collect();
    let union_defs: HashMap<String, UnionDef> = program
        .unions
        .iter()
        .map(|u| (u.name.clone(), u.clone()))
        .collect();
    let struct_defs: HashMap<String, StructDef> = program
        .structs
        .iter()
        .map(|s| (s.name.clone(), s.clone()))
        .collect();
    with_type_defs(
        &enum_defs,
        &error_set_defs,
        &union_defs,
        &struct_defs,
        || {},
    );

    for e in &program.enums {
        emit_enum_def(&mut out, e)?;
        out.push('\n');
    }

    for e in &program.error_sets {
        emit_error_set_tag_def(&mut out, e)?;
        out.push('\n');
    }

    let mut emitted_error_unions: HashSet<String> = HashSet::new();
    for ty in collect_error_union_types(program) {
        emit_error_union_struct_def(&mut out, &ty, &mut emitted_error_unions)?;
        out.push('\n');
    }

    let mut emitted_optionals: HashSet<String> = HashSet::new();
    for ty in collect_optional_types(program) {
        emit_optional_struct_def(&mut out, &ty, &mut emitted_optionals)?;
        out.push('\n');
    }

    let mut emitted_slices: HashSet<String> = HashSet::new();
    for ty in collect_slice_types(program) {
        emit_slice_struct_def(&mut out, &ty, &mut emitted_slices)?;
        out.push('\n');
    }

    for s in &program.structs {
        emit_struct_def(&mut out, s)?;
        out.push('\n');
    }

    for u in &program.unions {
        emit_union_def(&mut out, u)?;
        out.push('\n');
    }

    let global_types: HashMap<String, Type> = program
        .globals
        .iter()
        .map(|g| (g.name.clone(), g.ty.clone()))
        .collect();
    for g in &program.globals {
        emit_global_var(&mut out, g, &global_types)?;
    }
    if !program.globals.is_empty() {
        out.push('\n');
    }

    for f in &program.functions {
        let _ = writeln!(out, "{};", prototype(f));
    }
    out.push('\n');

    let func_returns: HashMap<String, Type> = program
        .functions
        .iter()
        .map(|f| (f.name.clone(), f.return_type.clone()))
        .collect();

    let func_params: HashMap<String, Vec<Type>> = program
        .functions
        .iter()
        .map(|f| {
            (
                f.name.clone(),
                f.params.iter().map(|(_, t)| t.clone()).collect(),
            )
        })
        .collect();
    FUNC_PARAMS.with(|cell| *cell.borrow_mut() = func_params);

    with_type_defs(
        &enum_defs,
        &error_set_defs,
        &union_defs,
        &struct_defs,
        || {
            for f in &program.functions {
                emit_function(&mut out, f, &func_returns, &global_types)?;
                out.push('\n');
            }
            Ok::<(), String>(())
        },
    )?;

    let void_main = program
        .functions
        .iter()
        .any(|f| f.name == "main" && f.return_type == Type::Void);
    if void_main {
        out.push_str("int main(void) {\n    zig_main();\n    return 0;\n}\n");
    } else {
        out.push_str("int main(void) {\n    return (int)zig_main();\n}\n");
    }
    Ok(out)
}

fn c_fn(name: &str) -> String {
    format!("zig_{name}")
}

fn emit_struct_def(out: &mut String, s: &StructDef) -> Result<(), String> {
    let _ = writeln!(out, "typedef struct {{");
    for (field, ty) in &s.fields {
        if s.packed {
            if let Some(bits) = packed_field_bits(ty) {
                let _ = writeln!(out, "    {} {} : {};", packed_field_c_type(ty), field, bits);
                continue;
            }
        }
        let _ = writeln!(out, "    {} {};", c_type(ty), field);
    }
    let _ = writeln!(out, "}} {};", s.name);
    Ok(())
}

fn c_union_tag(union: &UnionDef, variant: &str) -> String {
    match &union.tag_enum {
        Some(enum_name) => c_enum_variant(enum_name, variant),
        None => format!("{}_tag_{}", union.name, variant),
    }
}

fn union_tag_c_type(union: &UnionDef) -> String {
    union
        .tag_enum
        .clone()
        .unwrap_or_else(|| format!("{}_tag", union.name))
}

fn union_variant_payload_type(u: &UnionDef, variant: &str) -> Result<Type, String> {
    u.variants
        .iter()
        .find(|v| v.name == variant)
        .and_then(|v| v.payload.clone())
        .ok_or_else(|| format!("unknown union variant {variant:?} for {}", u.name))
}

fn union_payload_decl(variants: &[UnionVariant]) -> String {
    let mut fields = Vec::new();
    for v in variants {
        if let Some(ty) = &v.payload {
            fields.push(format!("{} {}", c_type(ty), v.name));
        }
    }
    if fields.is_empty() {
        fields.push("uint8_t _void".to_string());
    }
    format!("union {{ {}; }}", fields.join("; "))
}

fn union_payload_access(base: &str, variant: &str) -> String {
    format!("({base}).payload.{variant}")
}

fn emit_union_def(out: &mut String, u: &UnionDef) -> Result<(), String> {
    if u.tag_enum.is_none() {
        let _ = writeln!(out, "typedef enum {{");
        for v in &u.variants {
            let _ = writeln!(out, "    {},", c_union_tag(u, &v.name));
        }
        let _ = writeln!(out, "}} {}_tag;", u.name);
    }
    let tag_ty = union_tag_c_type(u);
    let payload_ty = union_payload_decl(&u.variants);
    let _ = writeln!(
        out,
        "typedef struct {{ {tag_ty} tag; {payload_ty} payload; }} {};",
        u.name
    );
    Ok(())
}

fn collect_optional_types(program: &Program) -> Vec<Type> {
    let mut out = Vec::new();
    let mut add = |ty: &Type| {
        if let Type::Optional(_) = ty {
            if !out.iter().any(|t| t == ty) {
                out.push(ty.clone());
            }
        }
    };
    for f in &program.functions {
        add(&f.return_type);
        for (_, ty) in &f.params {
            add(ty);
        }
        collect_optionals_in_stmts(&f.body, &mut add);
    }
    for g in &program.globals {
        add(&g.ty);
        collect_optionals_in_expr(&g.value, &mut add);
    }
    out
}

fn collect_optionals_in_stmts(stmts: &[Stmt], add: &mut dyn FnMut(&Type)) {
    for s in stmts {
        match s {
            Stmt::Let { ty, value, .. } => {
                add(ty);
                collect_optionals_in_expr(value, add);
            }
            Stmt::Assign { value, .. } => collect_optionals_in_expr(value, add),
            Stmt::Return(e) => collect_optionals_in_expr(e, add),
            Stmt::If {
                cond,
                ok_capture: _,
                then_branch,
                err_capture: _,
                else_branch,
            } => {
                collect_optionals_in_expr(cond, add);
                collect_optionals_in_stmts(then_branch, add);
                if let Some(eb) = else_branch {
                    collect_optionals_in_stmts(eb, add);
                }
            }
            Stmt::While {
                cond, cont, body, ..
            } => {
                collect_optionals_in_expr(cond, add);
                if let Some(c) = cont {
                    collect_optionals_in_expr(c, add);
                }
                collect_optionals_in_stmts(body, add);
            }
            Stmt::ForRange {
                start, end, body, ..
            } => {
                collect_optionals_in_expr(start, add);
                collect_optionals_in_expr(end, add);
                collect_optionals_in_stmts(body, add);
            }
            Stmt::ForArray { body, .. } => collect_optionals_in_stmts(body, add),
            Stmt::Break { .. } | Stmt::Continue => {}
            Stmt::Expr(e) => collect_optionals_in_expr(e, add),
            Stmt::Switch { scrutinee, arms } => {
                collect_optionals_in_expr(scrutinee, add);
                for arm in arms {
                    collect_optionals_in_stmts(&arm.body, add);
                }
            }
        }
    }
}

fn collect_optionals_in_expr(expr: &Expr, add: &mut dyn FnMut(&Type)) {
    match expr {
        Expr::BinOp { left, right, .. } => {
            collect_optionals_in_expr(left, add);
            collect_optionals_in_expr(right, add);
        }
        Expr::Call { args, .. } => {
            for a in args {
                collect_optionals_in_expr(a, add);
            }
        }
        Expr::Orelse { left, right } => {
            collect_optionals_in_expr(left, add);
            collect_optionals_in_expr(right, add);
        }
        Expr::Try(e) => collect_optionals_in_expr(e, add),
        Expr::Catch { expr, fallback, .. }
        | Expr::CatchReturn {
            expr,
            ret_val: fallback,
            ..
        } => {
            collect_optionals_in_expr(expr, add);
            collect_optionals_in_expr(fallback, add);
        }
        Expr::If {
            cond,
            then_expr,
            else_expr,
        } => {
            collect_optionals_in_expr(cond, add);
            collect_optionals_in_expr(then_expr, add);
            collect_optionals_in_expr(else_expr, add);
        }
        Expr::Switch {
            scrutinee,
            arms,
            default,
        } => {
            collect_optionals_in_expr(scrutinee, add);
            for arm in arms {
                collect_optionals_in_expr(&arm.expr, add);
            }
            if let Some(d) = default {
                collect_optionals_in_expr(d, add);
            }
        }
        Expr::IntCast { expr, .. }
        | Expr::BitCast { expr, .. }
        | Expr::UnaryNeg(expr)
        | Expr::UnaryNot(expr) => {
            collect_optionals_in_expr(expr, add);
        }
        Expr::Mod { left, right } | Expr::Rem { left, right } => {
            collect_optionals_in_expr(left, add);
            collect_optionals_in_expr(right, add);
        }
        Expr::ArrayLiteral { elems, .. } => {
            for e in elems {
                collect_optionals_in_expr(e, add);
            }
        }
        Expr::Index { base, index } => {
            collect_optionals_in_expr(base, add);
            collect_optionals_in_expr(index, add);
        }
        Expr::SliceRange { base, start, end } => {
            collect_optionals_in_expr(base, add);
            collect_optionals_in_expr(start, add);
            collect_optionals_in_expr(end, add);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                collect_optionals_in_expr(e, add);
            }
        }
        Expr::UnionLiteral { value: Some(v), .. } => collect_optionals_in_expr(v, add),
        Expr::FieldAccess { base, .. } => collect_optionals_in_expr(base, add),
        Expr::IntFromEnum(inner)
        | Expr::Deref(inner)
        | Expr::OptionalUnwrap(inner)
        | Expr::AddrOf(inner) => {
            collect_optionals_in_expr(inner, add);
        }
        Expr::DebugPrint { args, .. } => {
            for arg in args {
                collect_optionals_in_expr(arg, add);
            }
        }
        Expr::Int(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::TypeValue(_)
        | Expr::StringLit(_)
        | Expr::Undefined
        | Expr::Var(_)
        | Expr::EnumLiteral { .. }
        | Expr::ErrorLiteral { .. }
        | Expr::UnionLiteral { value: None, .. }
        | Expr::EmptyInit => {}
    }
}

fn collect_error_union_types(program: &Program) -> Vec<Type> {
    let mut out = Vec::new();
    let mut add = |ty: &Type| {
        if let Type::ErrorUnion { .. } = ty {
            if !out.iter().any(|t| t == ty) {
                out.push(ty.clone());
            }
        }
    };
    for f in &program.functions {
        add(&f.return_type);
        for (_, ty) in &f.params {
            add(ty);
        }
        collect_error_unions_in_stmts(&f.body, &mut add);
    }
    for g in &program.globals {
        add(&g.ty);
        collect_error_unions_in_expr(&g.value, &mut add);
    }
    out
}

fn collect_error_unions_in_stmts(stmts: &[Stmt], add: &mut dyn FnMut(&Type)) {
    for s in stmts {
        match s {
            Stmt::Let { ty, value, .. } => {
                add(ty);
                collect_error_unions_in_expr(value, add);
            }
            Stmt::Assign { value, .. } => collect_error_unions_in_expr(value, add),
            Stmt::Return(e) => collect_error_unions_in_expr(e, add),
            Stmt::If {
                cond,
                ok_capture: _,
                then_branch,
                err_capture: _,
                else_branch,
            } => {
                collect_error_unions_in_expr(cond, add);
                collect_error_unions_in_stmts(then_branch, add);
                if let Some(eb) = else_branch {
                    collect_error_unions_in_stmts(eb, add);
                }
            }
            Stmt::While {
                cond, cont, body, ..
            } => {
                collect_error_unions_in_expr(cond, add);
                if let Some(c) = cont {
                    collect_error_unions_in_expr(c, add);
                }
                collect_error_unions_in_stmts(body, add);
            }
            Stmt::ForRange {
                start, end, body, ..
            } => {
                collect_error_unions_in_expr(start, add);
                collect_error_unions_in_expr(end, add);
                collect_error_unions_in_stmts(body, add);
            }
            Stmt::ForArray { body, .. } => collect_error_unions_in_stmts(body, add),
            Stmt::Break { .. } | Stmt::Continue => {}
            Stmt::Expr(e) => collect_error_unions_in_expr(e, add),
            Stmt::Switch { scrutinee, arms } => {
                collect_error_unions_in_expr(scrutinee, add);
                for arm in arms {
                    collect_error_unions_in_stmts(&arm.body, add);
                }
            }
        }
    }
}

fn collect_error_unions_in_expr(expr: &Expr, add: &mut dyn FnMut(&Type)) {
    match expr {
        Expr::BinOp { left, right, .. } => {
            collect_error_unions_in_expr(left, add);
            collect_error_unions_in_expr(right, add);
        }
        Expr::Call { args, .. } => {
            for a in args {
                collect_error_unions_in_expr(a, add);
            }
        }
        Expr::Orelse { left, right } => {
            collect_error_unions_in_expr(left, add);
            collect_error_unions_in_expr(right, add);
        }
        Expr::Try(e) => collect_error_unions_in_expr(e, add),
        Expr::Catch { expr, fallback, .. }
        | Expr::CatchReturn {
            expr,
            ret_val: fallback,
            ..
        } => {
            collect_error_unions_in_expr(expr, add);
            collect_error_unions_in_expr(fallback, add);
        }
        Expr::If {
            cond,
            then_expr,
            else_expr,
        } => {
            collect_error_unions_in_expr(cond, add);
            collect_error_unions_in_expr(then_expr, add);
            collect_error_unions_in_expr(else_expr, add);
        }
        Expr::Switch {
            scrutinee,
            arms,
            default,
        } => {
            collect_error_unions_in_expr(scrutinee, add);
            for arm in arms {
                collect_error_unions_in_expr(&arm.expr, add);
            }
            if let Some(d) = default {
                collect_error_unions_in_expr(d, add);
            }
        }
        Expr::IntCast { expr, .. }
        | Expr::BitCast { expr, .. }
        | Expr::UnaryNeg(expr)
        | Expr::UnaryNot(expr) => {
            collect_error_unions_in_expr(expr, add);
        }
        Expr::Mod { left, right } | Expr::Rem { left, right } => {
            collect_error_unions_in_expr(left, add);
            collect_error_unions_in_expr(right, add);
        }
        Expr::ArrayLiteral { elems, .. } => {
            for e in elems {
                collect_error_unions_in_expr(e, add);
            }
        }
        Expr::Index { base, index } => {
            collect_error_unions_in_expr(base, add);
            collect_error_unions_in_expr(index, add);
        }
        Expr::SliceRange { base, start, end } => {
            collect_error_unions_in_expr(base, add);
            collect_error_unions_in_expr(start, add);
            collect_error_unions_in_expr(end, add);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                collect_error_unions_in_expr(e, add);
            }
        }
        Expr::UnionLiteral { value: Some(v), .. } => collect_error_unions_in_expr(v, add),
        Expr::UnionLiteral { value: None, .. } => {}
        Expr::FieldAccess { base, .. } => collect_error_unions_in_expr(base, add),
        Expr::IntFromEnum(inner)
        | Expr::Deref(inner)
        | Expr::OptionalUnwrap(inner)
        | Expr::AddrOf(inner) => {
            collect_error_unions_in_expr(inner, add);
        }
        Expr::DebugPrint { args, .. } => {
            for arg in args {
                collect_error_unions_in_expr(arg, add);
            }
        }
        Expr::Int(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::TypeValue(_)
        | Expr::StringLit(_)
        | Expr::Undefined
        | Expr::Var(_)
        | Expr::EnumLiteral { .. }
        | Expr::ErrorLiteral { .. }
        | Expr::EmptyInit => {}
    }
}

fn emit_error_set_tag_def(out: &mut String, e: &ErrorSetDef) -> Result<(), String> {
    let tag_ty = c_error_tag_enum(&e.name);
    let _ = writeln!(out, "typedef enum {{");
    let _ = writeln!(out, "    {} = 0,", c_error_ok_tag(&e.name));
    for v in &e.variants {
        let _ = writeln!(out, "    {},", c_error_variant_tag(&e.name, v));
    }
    let _ = writeln!(out, "}} {tag_ty};");
    Ok(())
}

fn emit_error_union_struct_def(
    out: &mut String,
    ty: &Type,
    emitted: &mut HashSet<String>,
) -> Result<(), String> {
    let Type::ErrorUnion { err_set, payload } = ty else {
        return Ok(());
    };
    let name = c_error_union_name(err_set, payload);
    if !emitted.insert(name.clone()) {
        return Ok(());
    }
    let tag_ty = c_error_tag_enum(err_set);
    let payload_ct = c_type(payload);
    let _ = writeln!(
        out,
        "typedef struct {{ {tag_ty} err; {payload_ct} value; }} {name};"
    );
    Ok(())
}

fn c_error_tag_enum(err_set: &str) -> String {
    format!("{err_set}_err")
}

fn c_error_ok_tag(err_set: &str) -> String {
    format!("{err_set}_err_ok")
}

fn c_error_variant_tag(err_set: &str, variant: &str) -> String {
    format!("{err_set}_err_{variant}")
}

fn map_error_tag_expr(
    src_tag: &str,
    src_err_set: &str,
    dest_err_set: &str,
) -> Result<String, String> {
    if src_err_set == dest_err_set {
        return Ok(src_tag.to_string());
    }
    let src = lookup_error_set(src_err_set)
        .ok_or_else(|| format!("unknown source error set {src_err_set:?}"))?;
    let dest = lookup_error_set(dest_err_set)
        .ok_or_else(|| format!("unknown destination error set {dest_err_set:?}"))?;
    let mut expr = c_error_ok_tag(dest_err_set);
    for variant in src.variants.iter().rev() {
        if !dest.variants.iter().any(|v| v == variant) {
            return Err(format!(
                "cannot coerce error {src_err_set}.{variant} into {dest_err_set}"
            ));
        }
        expr = format!(
            "(({src_tag}) == {} ? {} : ({expr}))",
            c_error_variant_tag(src_err_set, variant),
            c_error_variant_tag(dest_err_set, variant)
        );
    }
    Ok(expr)
}

fn error_union_error_value(
    src_tag: &str,
    src_err_set: &str,
    dest_err_set: &str,
    payload: &Type,
) -> Result<String, String> {
    let st = c_error_union_name(dest_err_set, payload);
    let mapped = map_error_tag_expr(src_tag, src_err_set, dest_err_set)?;
    Ok(format!("({st}){{ .err = {mapped} }}"))
}

fn c_error_union_name(err_set: &str, payload: &Type) -> String {
    format!("{err_set}_{}", c_type(payload))
}

fn c_optional_name(inner: &Type) -> String {
    format!("Opt_{}", c_type(inner))
}

fn emit_optional_struct_def(
    out: &mut String,
    ty: &Type,
    emitted: &mut HashSet<String>,
) -> Result<(), String> {
    let Type::Optional(inner) = ty else {
        return Ok(());
    };
    let name = c_optional_name(inner);
    if !emitted.insert(name.clone()) {
        return Ok(());
    }
    let payload_ct = c_type(inner);
    let _ = writeln!(
        out,
        "typedef struct {{ bool is_null; {payload_ct} value; }} {name};"
    );
    Ok(())
}

fn collect_slice_types(program: &Program) -> Vec<Type> {
    let mut out = Vec::new();
    let mut add = |ty: &Type| {
        if let Type::Slice { .. } = ty {
            if !out.iter().any(|t| t == ty) {
                out.push(ty.clone());
            }
        }
    };
    for f in &program.functions {
        add(&f.return_type);
        for (_, ty) in &f.params {
            add(ty);
        }
        collect_slices_in_stmts(&f.body, &mut add);
    }
    for g in &program.globals {
        add(&g.ty);
        collect_slices_in_expr(&g.value, &mut add);
    }
    out
}

fn collect_slices_in_stmts(stmts: &[Stmt], add: &mut dyn FnMut(&Type)) {
    for s in stmts {
        match s {
            Stmt::Let { ty, value, .. } => {
                add(ty);
                collect_slices_in_expr(value, add);
            }
            Stmt::Assign { value, .. } => collect_slices_in_expr(value, add),
            Stmt::Return(e) => collect_slices_in_expr(e, add),
            Stmt::If {
                cond,
                then_branch,
                else_branch,
                ..
            } => {
                collect_slices_in_expr(cond, add);
                collect_slices_in_stmts(then_branch, add);
                if let Some(eb) = else_branch {
                    collect_slices_in_stmts(eb, add);
                }
            }
            Stmt::While {
                cond, cont, body, ..
            } => {
                collect_slices_in_expr(cond, add);
                if let Some(c) = cont {
                    collect_slices_in_expr(c, add);
                }
                collect_slices_in_stmts(body, add);
            }
            Stmt::ForRange {
                start, end, body, ..
            } => {
                collect_slices_in_expr(start, add);
                collect_slices_in_expr(end, add);
                collect_slices_in_stmts(body, add);
            }
            Stmt::ForArray { body, .. } => collect_slices_in_stmts(body, add),
            Stmt::Break { .. } | Stmt::Continue => {}
            Stmt::Expr(e) => collect_slices_in_expr(e, add),
            Stmt::Switch { scrutinee, arms } => {
                collect_slices_in_expr(scrutinee, add);
                for arm in arms {
                    collect_slices_in_stmts(&arm.body, add);
                }
            }
        }
    }
}

fn collect_slices_in_expr(expr: &Expr, add: &mut dyn FnMut(&Type)) {
    match expr {
        Expr::BinOp { left, right, .. } => {
            collect_slices_in_expr(left, add);
            collect_slices_in_expr(right, add);
        }
        Expr::Call { args, .. } => {
            for a in args {
                collect_slices_in_expr(a, add);
            }
        }
        Expr::Try(e) => collect_slices_in_expr(e, add),
        Expr::Catch { expr, fallback, .. } => {
            collect_slices_in_expr(expr, add);
            collect_slices_in_expr(fallback, add);
        }
        Expr::CatchReturn { expr, ret_val, .. } => {
            collect_slices_in_expr(expr, add);
            collect_slices_in_expr(ret_val, add);
        }
        Expr::If {
            cond,
            then_expr,
            else_expr,
        } => {
            collect_slices_in_expr(cond, add);
            collect_slices_in_expr(then_expr, add);
            collect_slices_in_expr(else_expr, add);
        }
        Expr::Switch {
            scrutinee,
            arms,
            default,
        } => {
            collect_slices_in_expr(scrutinee, add);
            for arm in arms {
                collect_slices_in_expr(&arm.expr, add);
            }
            if let Some(d) = default {
                collect_slices_in_expr(d, add);
            }
        }
        Expr::IntCast { expr, .. }
        | Expr::BitCast { expr, .. }
        | Expr::UnaryNeg(expr)
        | Expr::UnaryNot(expr)
        | Expr::AddrOf(expr) => {
            collect_slices_in_expr(expr, add);
        }
        Expr::Mod { left, right } | Expr::Rem { left, right } => {
            collect_slices_in_expr(left, add);
            collect_slices_in_expr(right, add);
        }
        Expr::ArrayLiteral { elems, .. } => {
            for e in elems {
                collect_slices_in_expr(e, add);
            }
        }
        Expr::Index { base, index } => {
            collect_slices_in_expr(base, add);
            collect_slices_in_expr(index, add);
        }
        Expr::SliceRange { base, start, end } => {
            collect_slices_in_expr(base, add);
            collect_slices_in_expr(start, add);
            collect_slices_in_expr(end, add);
        }
        Expr::StructLiteral { fields, .. } => {
            for (_, e) in fields {
                collect_slices_in_expr(e, add);
            }
        }
        Expr::UnionLiteral { value: Some(v), .. } => collect_slices_in_expr(v, add),
        Expr::FieldAccess { base, .. } => collect_slices_in_expr(base, add),
        Expr::IntFromEnum(inner) | Expr::Deref(inner) | Expr::OptionalUnwrap(inner) => {
            collect_slices_in_expr(inner, add)
        }
        Expr::Orelse { left, right } => {
            collect_slices_in_expr(left, add);
            collect_slices_in_expr(right, add);
        }
        Expr::DebugPrint { args, .. } => {
            for arg in args {
                collect_slices_in_expr(arg, add);
            }
        }
        Expr::Int(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::TypeValue(_)
        | Expr::StringLit(_)
        | Expr::Undefined
        | Expr::Var(_)
        | Expr::EnumLiteral { .. }
        | Expr::ErrorLiteral { .. }
        | Expr::UnionLiteral { value: None, .. }
        | Expr::EmptyInit => {}
    }
}

fn c_slice_name(ty: &Type) -> String {
    let Type::Slice { const_, elem } = ty else {
        return "Slice".to_string();
    };
    format!(
        "Slice_{}_{}",
        if *const_ { "const" } else { "mut" },
        c_type(elem).replace(' ', "_")
    )
}

fn c_slice_ptr_type(ty: &Type) -> String {
    let Type::Slice { const_, elem } = ty else {
        return "void *".to_string();
    };
    if *const_ {
        format!("{} const *", c_type(elem))
    } else {
        format!("{} *", c_type(elem))
    }
}

fn emit_slice_struct_def(
    out: &mut String,
    ty: &Type,
    emitted: &mut HashSet<String>,
) -> Result<(), String> {
    let Type::Slice { .. } = ty else {
        return Ok(());
    };
    let name = c_slice_name(ty);
    if !emitted.insert(name.clone()) {
        return Ok(());
    }
    let ptr_ct = c_slice_ptr_type(ty);
    let _ = writeln!(
        out,
        "typedef struct {{ {ptr_ct} ptr; size_t len; }} {name};"
    );
    Ok(())
}

fn array_to_slice_expr(arr_expr: &str, len: &ArrayLen, slice_ty: &Type) -> String {
    format!(
        "({}){{ .ptr = {arr_expr}, .len = {} }}",
        c_slice_name(slice_ty),
        len
    )
}

fn emit_enum_def(out: &mut String, e: &EnumDef) -> Result<(), String> {
    let _ = writeln!(out, "typedef enum {{");
    let mut next: i64 = 0;
    for v in &e.variants {
        let val = v.value.unwrap_or(next);
        next = val.saturating_add(1);
        let _ = writeln!(out, "    {}_{} = {},", e.name, v.name, val);
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
        Type::Int(IntType::U1)
        | Type::Int(IntType::U2)
        | Type::Int(IntType::U3)
        | Type::Int(IntType::U4)
        | Type::Int(IntType::U5) => "uint8_t".to_string(),
        Type::Int(IntType::U8) => "uint8_t".to_string(),
        Type::Int(IntType::U16) => "uint16_t".to_string(),
        Type::Int(IntType::U32) => "uint32_t".to_string(),
        Type::Int(IntType::U64) => "uint64_t".to_string(),
        Type::Int(IntType::I8) => "int8_t".to_string(),
        Type::Int(IntType::I16) => "int16_t".to_string(),
        Type::Int(IntType::I32) => "int32_t".to_string(),
        Type::Int(IntType::I64) => "int64_t".to_string(),
        Type::Array { .. } => "uint8_t".to_string(),
        Type::Slice { .. } => c_slice_name(ty),
        Type::Enum(name) => name.clone(),
        Type::Struct(name) => name.clone(),
        Type::Union(name) => name.clone(),
        Type::ErrorUnion { err_set, payload } => c_error_union_name(err_set, payload),
        Type::Optional(inner) => c_optional_name(inner),
        Type::Pointer(inner) => format!("{} *", c_type(inner)),
        Type::Type => "void *".to_string(),
        Type::Generic(name) => name.clone(),
        Type::Void => "void".to_string(),
    }
}

fn c_var_decl(name: &str, ty: &Type) -> String {
    match ty {
        Type::Array { .. } => format!("{} {name}{}", c_array_base(ty), c_array_suffix(ty)),
        Type::Struct(_) | Type::Union(_) => format!("{} {name}", c_type(ty)),
        other => format!("{} {name}", c_type(other)),
    }
}

fn c_ptr_to_array_elem_decl(name: &str, array_ty: &Type) -> String {
    match array_ty {
        Type::Array { elem, .. } => {
            format!(
                "{} (*{name}){}",
                c_array_base(array_ty),
                c_array_suffix(elem)
            )
        }
        other => format!("{} *{name}", c_type(other)),
    }
}

fn c_array_base(ty: &Type) -> String {
    match ty {
        Type::Array { elem, .. } => match elem.as_ref() {
            Type::Array { .. } => c_array_base(elem),
            Type::Int(t) => c_int_type(*t),
            Type::Union(name) => name.clone(),
            other => c_type(other),
        },
        other => c_type(other),
    }
}

fn c_array_suffix(ty: &Type) -> String {
    match ty {
        Type::Array { len, elem } => format!("[{len}]{}", c_array_suffix(elem)),
        _ => String::new(),
    }
}

fn c_int_type(ty: IntType) -> String {
    c_type(&Type::Int(ty))
}

fn c_string_literal(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn emit_debug_print_call(
    format: &str,
    args: &[Expr],
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
    fn_return_type: &Type,
    temp_id: &mut usize,
) -> Result<String, String> {
    let mut c_format = String::new();
    let mut c_args = Vec::new();
    let mut arg_idx = 0usize;
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0usize;
    while i < chars.len() {
        match chars[i] {
            '{' if chars.get(i + 1) == Some(&'{') => {
                c_format.push('{');
                i += 2;
            }
            '}' if chars.get(i + 1) == Some(&'}') => {
                c_format.push('}');
                i += 2;
            }
            '{' => {
                let start = i + 1;
                let end = chars[start..]
                    .iter()
                    .position(|c| *c == '}')
                    .map(|off| start + off)
                    .ok_or_else(|| "unterminated std.debug.print format field".to_string())?;
                let spec: String = chars[start..end].iter().collect();
                let arg = args.get(arg_idx).ok_or_else(|| {
                    "std.debug.print has fewer arguments than format fields".to_string()
                })?;
                arg_idx += 1;
                let arg_ty = expr_type(arg, env, func_returns);
                match (spec.as_str(), &arg_ty) {
                    ("", Type::Bool) => {
                        c_format.push_str("%s");
                        let rendered = emit_expr(
                            arg,
                            env,
                            Some(&Type::Bool),
                            func_returns,
                            fn_return_type,
                            temp_id,
                        )?;
                        c_args.push(format!("({rendered}) ? \"true\" : \"false\""));
                    }
                    ("", Type::Int(it)) => {
                        if it.is_signed() {
                            c_format.push_str("%lld");
                            let rendered = emit_expr(
                                arg,
                                env,
                                Some(&arg_ty),
                                func_returns,
                                fn_return_type,
                                temp_id,
                            )?;
                            c_args.push(format!("(long long)({rendered})"));
                        } else {
                            c_format.push_str("%llu");
                            let rendered = emit_expr(
                                arg,
                                env,
                                Some(&arg_ty),
                                func_returns,
                                fn_return_type,
                                temp_id,
                            )?;
                            c_args.push(format!("(unsigned long long)({rendered})"));
                        }
                    }
                    ("", Type::Enum(_)) => {
                        c_format.push_str("%lld");
                        let rendered =
                            emit_expr(arg, env, None, func_returns, fn_return_type, temp_id)?;
                        c_args.push(format!("(long long)({rendered})"));
                    }
                    ("s", Type::Slice { .. }) => {
                        let rendered = emit_expr(
                            arg,
                            env,
                            Some(&arg_ty),
                            func_returns,
                            fn_return_type,
                            temp_id,
                        )?;
                        c_format.push_str("%.*s");
                        c_args.push(format!("(int)({rendered}).len"));
                        c_args.push(format!("(const char *)({rendered}).ptr"));
                    }
                    ("s", _) => {
                        let rendered =
                            emit_expr(arg, env, None, func_returns, fn_return_type, temp_id)?;
                        c_format.push_str("%s");
                        c_args.push(rendered);
                    }
                    _ => {
                        return Err(format!(
                            "unsupported std.debug.print format field {{{spec}}} for {arg_ty:?}"
                        ));
                    }
                }
                i = end + 1;
            }
            '%' => {
                c_format.push_str("%%");
                i += 1;
            }
            ch => {
                c_format.push(ch);
                i += 1;
            }
        }
    }
    if arg_idx != args.len() {
        return Err("std.debug.print has more arguments than format fields".to_string());
    }
    let mut call = format!("fprintf(stderr, {}", c_string_literal(&c_format));
    for arg in c_args {
        call.push_str(", ");
        call.push_str(&arg);
    }
    call.push(')');
    Ok(call)
}

fn prototype(f: &Function) -> String {
    let params = if f.params.is_empty() {
        "void".to_string()
    } else {
        f.params
            .iter()
            .map(|(p, ty)| c_var_decl(p, ty))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("{} {}({})", c_type(&f.return_type), c_fn(&f.name), params)
}

fn emit_global_var(
    out: &mut String,
    g: &GlobalVar,
    global_types: &HashMap<String, Type>,
) -> Result<(), String> {
    let mut temp_id = 0usize;
    let init = emit_expr(
        &g.value,
        global_types,
        Some(&g.ty),
        &HashMap::new(),
        &Type::Void,
        &mut temp_id,
    )?;
    let _ = writeln!(out, "{} = {};", c_var_decl(&g.name, &g.ty), init);
    Ok(())
}

fn emit_function(
    out: &mut String,
    f: &Function,
    func_returns: &HashMap<String, Type>,
    global_types: &HashMap<String, Type>,
) -> Result<(), String> {
    let _ = writeln!(out, "{} {{", prototype(f));
    let mut env: HashMap<String, Type> = global_types.clone();
    let mut temp_id = 0usize;
    for (name, ty) in &f.params {
        env.insert(name.clone(), ty.clone());
    }
    let mut loop_cont: Vec<Option<String>> = Vec::new();
    let mut loop_break_labels: Vec<(String, String)> = Vec::new();
    for s in &f.body {
        emit_stmt(
            out,
            s,
            1,
            &mut env,
            &f.return_type,
            func_returns,
            &mut temp_id,
            &mut loop_cont,
            &mut loop_break_labels,
        )?;
    }
    out.push_str("}\n");
    Ok(())
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn indexed_lvalue_type(
    base: &Expr,
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
) -> Type {
    expr_type(base, env, func_returns)
        .index_result_type()
        .unwrap_or(Type::Int(IntType::U8))
}

fn expr_type(
    expr: &Expr,
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
) -> Type {
    match expr {
        Expr::Int(_) => Type::Int(IntType::U8),
        Expr::Bool(_) => Type::Bool,
        Expr::Null => Type::Int(IntType::U8),
        Expr::TypeValue(_) => Type::Type,
        Expr::Undefined => Type::Int(IntType::U8),
        Expr::Var(name) => env.get(name).cloned().unwrap_or(Type::Int(IntType::U8)),
        Expr::EnumLiteral {
            enum_name,
            variant: _,
        } => Type::Enum(enum_name.clone()),
        Expr::IntFromEnum(inner) => {
            let backing = expr_type(inner, env, func_returns)
                .enum_name()
                .and_then(|n| lookup_enum(n))
                .and_then(|e| e.backing)
                .unwrap_or(IntType::U8);
            Type::Int(backing)
        }
        Expr::ErrorLiteral { .. } => Type::Int(IntType::U8),
        Expr::Try(inner) => expr_type(inner, env, func_returns)
            .error_union_payload()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::Catch { expr, .. } => expr_type(expr, env, func_returns)
            .error_union_payload()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::CatchReturn { expr, .. } => expr_type(expr, env, func_returns)
            .error_union_payload()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::BinOp { op, left, right } => match op {
            BinOp::Lt
            | BinOp::Gt
            | BinOp::Le
            | BinOp::Ge
            | BinOp::Eq
            | BinOp::Ne
            | BinOp::LogicalAnd
            | BinOp::LogicalOr => Type::Bool,
            _ => combine_types(
                expr_type(left, env, func_returns),
                expr_type(right, env, func_returns),
            ),
        },
        Expr::Call { name, .. } => func_returns
            .get(name)
            .cloned()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::If {
            then_expr,
            else_expr,
            ..
        } => combine_types(
            expr_type(then_expr, env, func_returns),
            expr_type(else_expr, env, func_returns),
        ),
        Expr::Switch { default, arms, .. } => default
            .as_ref()
            .map(|d| expr_type(d, env, func_returns))
            .or_else(|| arms.last().map(|a| expr_type(&a.expr, env, func_returns)))
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::IntCast { target, .. } => Type::Int(*target),
        Expr::BitCast { target, .. } => target.clone(),
        Expr::Mod { left, right } | Expr::Rem { left, right } => combine_types(
            expr_type(left, env, func_returns),
            expr_type(right, env, func_returns),
        ),
        Expr::UnaryNeg(inner) => expr_type(inner, env, func_returns),
        Expr::UnaryNot(_) => Type::Bool,
        Expr::ArrayLiteral { elems, annotated } => {
            if let Some((len_opt, elem)) = annotated {
                Type::Array {
                    len: ArrayLen::Known(len_opt.unwrap_or(elems.len())),
                    elem: Box::new(elem.clone()),
                }
            } else if let Some(first) = elems.first() {
                let elem = expr_type(first, env, func_returns)
                    .int_type()
                    .unwrap_or(IntType::U8);
                Type::Array {
                    len: ArrayLen::Known(elems.len()),
                    elem: Box::new(Type::Int(elem)),
                }
            } else {
                Type::Array {
                    len: ArrayLen::Known(0),
                    elem: Box::new(Type::Int(IntType::U8)),
                }
            }
        }
        Expr::Index { base, .. } => indexed_lvalue_type(base, env, func_returns),
        Expr::SliceRange { base, .. } => {
            let base_ty = expr_type(base, env, func_returns);
            match base_ty {
                Type::Slice { .. } => base_ty,
                Type::Array { elem, .. } => Type::Slice {
                    const_: false,
                    elem,
                },
                _ => Type::Slice {
                    const_: false,
                    elem: Box::new(Type::Int(IntType::U8)),
                },
            }
        }
        Expr::StructLiteral { struct_name, .. } => Type::Struct(struct_name.clone()),
        Expr::UnionLiteral { union_name, .. } => {
            Type::Union(union_name.clone().unwrap_or_else(|| "Shape".to_string()))
        }
        Expr::EmptyInit => Type::Void,
        Expr::FieldAccess { base, field } => field_expr_type(base, field, env),
        Expr::Deref(inner) => expr_type(inner, env, func_returns)
            .pointee()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::OptionalUnwrap(inner) => expr_type(inner, env, func_returns)
            .optional_inner()
            .unwrap_or(Type::Int(IntType::U8)),
        Expr::AddrOf(inner) => Type::Pointer(Box::new(expr_type(inner, env, func_returns))),
        Expr::Orelse { left, right } => expr_type(left, env, func_returns)
            .optional_inner()
            .unwrap_or_else(|| expr_type(right, env, func_returns)),
        Expr::StringLit(_) => Type::Slice {
            const_: true,
            elem: Box::new(Type::Int(IntType::U8)),
        },
        Expr::DebugPrint { .. } => Type::Void,
    }
}

fn field_expr_type(base: &Expr, field: &str, env: &HashMap<String, Type>) -> Type {
    let base_ty = match base {
        Expr::Var(name) => env.get(name).cloned(),
        Expr::FieldAccess {
            base,
            field: parent,
        } => Some(field_expr_type(base, parent, env)),
        Expr::StructLiteral { struct_name, .. } => Some(Type::Struct(struct_name.clone())),
        _ => None,
    };
    if base_ty
        .as_ref()
        .is_some_and(|t| matches!(t, Type::Array { .. } | Type::Slice { .. }))
        && field == "len"
    {
        return Type::Int(IntType::U64);
    }
    if let Some(Type::Union(ref union_name)) = base_ty {
        if let Some(udef) = lookup_union(union_name) {
            if let Ok(ty) = union_variant_payload_type(&udef, field) {
                return ty;
            }
        }
    }
    if let Some(sn) = base_ty.and_then(|t| t.struct_name().map(str::to_string)) {
        if let Some(sdef) = lookup_struct(&sn) {
            if let Some((_, ty)) = sdef.fields.iter().find(|(f, _)| f == field) {
                return ty.clone();
            }
        }
    }
    Type::Int(IntType::U8)
}

fn combine_types(a: Type, b: Type) -> Type {
    match (a, b) {
        (Type::Int(x), Type::Int(y)) => Type::Int(wider_int_type(x, y)),
        (Type::Int(x), _) => Type::Int(x),
        (_, Type::Int(y)) => Type::Int(y),
        (Type::Array { elem, .. }, Type::Array { elem: elem2, .. }) => Type::Array {
            len: ArrayLen::Known(0),
            elem: Box::new(combine_types(elem.as_ref().clone(), elem2.as_ref().clone())),
        },
        (Type::Array { elem, .. }, _) | (_, Type::Array { elem, .. }) => elem
            .int_type()
            .map(Type::Int)
            .unwrap_or_else(|| elem.as_ref().clone()),
        (Type::Enum(a), Type::Enum(b)) if a == b => Type::Enum(a),
        (Type::Enum(a), _) | (_, Type::Enum(a)) => Type::Enum(a),
        (Type::Struct(a), Type::Struct(b)) if a == b => Type::Struct(a),
        (Type::Union(a), Type::Union(b)) if a == b => Type::Union(a),
        (Type::Union(a), _) | (_, Type::Union(a)) => Type::Union(a),
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

fn emit_while_cont_expr(
    out: &mut String,
    expr: &Expr,
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
    return_type: &Type,
    temp_id: &mut usize,
) -> Result<(), String> {
    if let Expr::BinOp { op, left, right } = expr {
        if let Expr::Var(name) = left.as_ref() {
            let ty = env.get(name).cloned().unwrap_or(Type::Int(IntType::U8));
            let _ = writeln!(
                out,
                "{name} = {};",
                emit_expr(
                    &Expr::BinOp {
                        op: *op,
                        left: Box::new(Expr::Var(name.clone())),
                        right: right.clone(),
                    },
                    env,
                    Some(&ty),
                    func_returns,
                    return_type,
                    temp_id,
                )?
            );
            return Ok(());
        }
    }
    let _ = writeln!(
        out,
        "{};",
        emit_expr(expr, env, None, func_returns, return_type, temp_id)?
    );
    Ok(())
}

fn emit_stmt(
    out: &mut String,
    stmt: &Stmt,
    depth: usize,
    env: &mut HashMap<String, Type>,
    return_type: &Type,
    func_returns: &HashMap<String, Type>,
    temp_id: &mut usize,
    loop_cont: &mut Vec<Option<String>>,
    loop_break_labels: &mut Vec<(String, String)>,
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
                    emit_expr(value, env, Some(ty), func_returns, return_type, temp_id)?
                );
            }
            env.insert(name.clone(), ty.clone());
        }
        Stmt::Assign { target, value } => {
            if let AssignTarget::Name(n) = target {
                if n == "_" {
                    let val_s = emit_expr(value, env, None, func_returns, return_type, temp_id)?;
                    let _ = writeln!(out, "(void)({val_s});");
                    return Ok(());
                }
            }
            let ty = assign_target_type(target, env);
            let lhs = emit_assign_target(target, env, func_returns, return_type, temp_id)?;
            let _ = writeln!(
                out,
                "{lhs} = {};",
                emit_expr(value, env, Some(&ty), func_returns, return_type, temp_id)?
            );
        }
        Stmt::Expr(Expr::DebugPrint { format, args }) => {
            let call =
                emit_debug_print_call(format, args, env, func_returns, return_type, temp_id)?;
            let _ = writeln!(out, "{call};");
        }
        Stmt::Expr(e) => {
            let _ = writeln!(
                out,
                "{};",
                emit_expr(e, env, None, func_returns, return_type, temp_id)?
            );
        }
        Stmt::Return(e) => {
            if matches!(return_type, Type::Void) {
                let _ = writeln!(out, "return;");
                return Ok(());
            }
            let ret = if let Type::ErrorUnion { err_set, payload } = return_type {
                if let Expr::ErrorLiteral { variant, .. } = e {
                    let st = c_error_union_name(err_set, payload);
                    let tag = c_error_variant_tag(err_set, variant);
                    format!("({st}){{ .err = {tag} }}")
                } else {
                    let val = emit_expr(e, env, Some(payload), func_returns, return_type, temp_id)?;
                    let st = c_error_union_name(err_set, payload);
                    let ok = c_error_ok_tag(err_set);
                    format!("({st}){{ .err = {ok}, .value = {val} }}")
                }
            } else {
                emit_expr(
                    e,
                    env,
                    Some(return_type),
                    func_returns,
                    return_type,
                    temp_id,
                )?
            };
            let _ = writeln!(out, "return {ret};");
        }
        Stmt::If {
            cond,
            ok_capture,
            then_branch,
            err_capture,
            else_branch,
        } => {
            if ok_capture.is_some() || err_capture.is_some() {
                let cond_ty = expr_type(cond, env, func_returns);
                if let Type::ErrorUnion { err_set, payload } = cond_ty.clone() {
                    let st = c_error_union_name(&err_set, &payload);
                    let ok = c_error_ok_tag(&err_set);
                    let tmp = next_temp(temp_id);
                    let cond_code = emit_expr(cond, env, None, func_returns, return_type, temp_id)?;
                    let _ = writeln!(out, "{} {} = {};", st, tmp, cond_code);
                    let _ = writeln!(out, "if ({}.err == {}) {{", tmp, ok);

                    let mut then_env = env.clone();
                    if let Some(ok_var) = ok_capture {
                        let payload_ct = c_type(&payload);
                        let _ = writeln!(out, "    {} {} = {}.value;", payload_ct, ok_var, tmp);
                        then_env.insert(ok_var.clone(), (*payload).clone());
                    }

                    for s in then_branch {
                        emit_stmt(
                            out,
                            s,
                            depth + 1,
                            &mut then_env,
                            return_type,
                            func_returns,
                            temp_id,
                            loop_cont,
                            loop_break_labels,
                        )?;
                    }

                    indent(out, depth);
                    out.push('}');

                    if let Some(eb) = else_branch {
                        out.push_str(" else {\n");

                        let mut else_env = env.clone();
                        if let Some(err_var) = err_capture {
                            let tag_ty = c_error_tag_enum(&err_set);
                            let _ = writeln!(out, "    {} {} = {}.err;", tag_ty, err_var, tmp);
                            else_env
                                .insert(err_var.clone(), Type::Enum(format!("{}_err", err_set)));
                        }

                        for s in eb {
                            emit_stmt(
                                out,
                                s,
                                depth + 1,
                                &mut else_env,
                                return_type,
                                func_returns,
                                temp_id,
                                loop_cont,
                                loop_break_labels,
                            )?;
                        }
                        indent(out, depth);
                        out.push('}');
                    }
                    out.push('\n');
                } else if let Type::Optional(inner) = cond_ty {
                    if err_capture.is_some() {
                        return Err("optional if does not support else error capture".to_string());
                    }
                    let opt = c_optional_name(&inner);
                    let tmp = next_temp(temp_id);
                    let cond_code = emit_expr(
                        cond,
                        env,
                        Some(&Type::Optional(inner.clone())),
                        func_returns,
                        return_type,
                        temp_id,
                    )?;
                    let _ = writeln!(out, "{} {} = {};", opt, tmp, cond_code);
                    let _ = writeln!(out, "if (!{}.is_null) {{", tmp);

                    let mut then_env = env.clone();
                    if let Some(ok_var) = ok_capture {
                        let payload_ct = c_type(&inner);
                        let _ = writeln!(out, "    {} {} = {}.value;", payload_ct, ok_var, tmp);
                        then_env.insert(ok_var.clone(), (*inner).clone());
                    }

                    for s in then_branch {
                        emit_stmt(
                            out,
                            s,
                            depth + 1,
                            &mut then_env,
                            return_type,
                            func_returns,
                            temp_id,
                            loop_cont,
                            loop_break_labels,
                        )?;
                    }

                    indent(out, depth);
                    out.push('}');

                    if let Some(eb) = else_branch {
                        out.push_str(" else {\n");
                        let mut else_env = env.clone();
                        for s in eb {
                            emit_stmt(
                                out,
                                s,
                                depth + 1,
                                &mut else_env,
                                return_type,
                                func_returns,
                                temp_id,
                                loop_cont,
                                loop_break_labels,
                            )?;
                        }
                        indent(out, depth);
                        out.push('}');
                    }
                    out.push('\n');
                } else {
                    return Err("if captures require error union or optional condition".to_string());
                }
            } else {
                let _ = writeln!(
                    out,
                    "if ({}) {{",
                    emit_expr(cond, env, None, func_returns, return_type, temp_id)?
                );
                for s in then_branch {
                    emit_stmt(
                        out,
                        s,
                        depth + 1,
                        env,
                        return_type,
                        func_returns,
                        temp_id,
                        loop_cont,
                        loop_break_labels,
                    )?;
                }
                indent(out, depth);
                out.push('}');
                if let Some(eb) = else_branch {
                    out.push_str(" else {\n");
                    for s in eb {
                        emit_stmt(
                            out,
                            s,
                            depth + 1,
                            env,
                            return_type,
                            func_returns,
                            temp_id,
                            loop_cont,
                            loop_break_labels,
                        )?;
                    }
                    indent(out, depth);
                    out.push('}');
                }
                out.push('\n');
            }
        }
        Stmt::While {
            cond,
            ok_capture,
            cont,
            body,
        } => {
            let cont_label = if cont.is_some() {
                let label = format!("zig_while_cont_{}", *temp_id);
                *temp_id += 1;
                loop_cont.push(Some(label.clone()));
                Some(label)
            } else {
                loop_cont.push(None);
                None
            };
            if let Some(ok_var) = ok_capture {
                let cond_ty = expr_type(cond, env, func_returns);
                let inner = cond_ty
                    .optional_inner()
                    .ok_or_else(|| "while capture requires optional condition".to_string())?;
                let opt_ty = Type::Optional(Box::new(inner.clone()));
                let opt = c_optional_name(&inner);
                let tmp = next_temp(temp_id);
                let cond_code =
                    emit_expr(cond, env, Some(&opt_ty), func_returns, return_type, temp_id)?;
                let _ = writeln!(out, "while (true) {{");
                indent(out, depth + 1);
                let _ = writeln!(out, "{opt} {tmp} = {cond_code};");
                indent(out, depth + 1);
                let _ = writeln!(out, "if ({tmp}.is_null) break;");
                let mut body_env = env.clone();
                body_env.insert(ok_var.clone(), inner.clone());
                indent(out, depth + 1);
                let _ = writeln!(out, "{} {} = {}.value;", c_type(&inner), ok_var, tmp);
                for s in body {
                    emit_stmt(
                        out,
                        s,
                        depth + 1,
                        &mut body_env,
                        return_type,
                        func_returns,
                        temp_id,
                        loop_cont,
                        loop_break_labels,
                    )?;
                }
                if let (Some(c), Some(label)) = (cont, cont_label.as_ref()) {
                    indent(out, depth + 1);
                    let _ = writeln!(out, "{label}:");
                    indent(out, depth + 1);
                    emit_while_cont_expr(out, c, &body_env, func_returns, return_type, temp_id)?;
                    out.push('\n');
                }
            } else {
                let _ = writeln!(
                    out,
                    "while ({}) {{",
                    emit_expr(cond, env, None, func_returns, return_type, temp_id)?
                );
                for s in body {
                    emit_stmt(
                        out,
                        s,
                        depth + 1,
                        env,
                        return_type,
                        func_returns,
                        temp_id,
                        loop_cont,
                        loop_break_labels,
                    )?;
                }
                if let (Some(c), Some(label)) = (cont, cont_label.as_ref()) {
                    indent(out, depth + 1);
                    let _ = writeln!(out, "{label}:");
                    indent(out, depth + 1);
                    emit_while_cont_expr(out, c, env, func_returns, return_type, temp_id)?;
                    out.push('\n');
                }
            }
            loop_cont.pop();
            indent(out, depth);
            out.push_str("}\n");
        }
        Stmt::Break { label, .. } => {
            if let Some(zig_label) = label {
                let c_label = loop_break_labels
                    .iter()
                    .rev()
                    .find(|(name, _)| name == zig_label)
                    .map(|(_, c)| c.clone())
                    .ok_or_else(|| format!("unknown loop label `{zig_label}`"))?;
                let _ = writeln!(out, "goto {c_label};");
            } else {
                let _ = writeln!(out, "break;");
            }
        }
        Stmt::Continue => {
            if let Some(Some(label)) = loop_cont.last() {
                let _ = writeln!(out, "goto {label};");
            } else {
                let _ = writeln!(out, "continue;");
            }
        }
        Stmt::ForRange {
            label,
            capture,
            start,
            end,
            body,
        } => {
            let var = capture.as_deref().unwrap_or("_zig_for_i");
            let loop_ty = combine_types(
                expr_type(start, env, func_returns),
                expr_type(end, env, func_returns),
            )
            .int_type()
            .unwrap_or(IntType::U8);
            let loop_ty_type = Type::Int(loop_ty);
            let end_label = label.as_ref().map(|zig_label| {
                let c_label = format!("zig_lbreak_{}", *temp_id);
                *temp_id += 1;
                loop_break_labels.push((zig_label.clone(), c_label.clone()));
                c_label
            });
            let _ = writeln!(
                out,
                "for ({} {var} = {}; {var} < {}; {var}++) {{",
                c_int_type(loop_ty),
                emit_expr(
                    start,
                    env,
                    Some(&loop_ty_type),
                    func_returns,
                    return_type,
                    temp_id
                )?,
                emit_expr(
                    end,
                    env,
                    Some(&loop_ty_type),
                    func_returns,
                    return_type,
                    temp_id
                )?
            );
            if let Some(cap) = capture {
                env.insert(cap.clone(), Type::Int(loop_ty));
            }
            for s in body {
                emit_stmt(
                    out,
                    s,
                    depth + 1,
                    env,
                    return_type,
                    func_returns,
                    temp_id,
                    loop_cont,
                    loop_break_labels,
                )?;
            }
            if let Some(cap) = capture {
                env.remove(cap);
            }
            indent(out, depth);
            out.push_str("}\n");
            if let Some(c_label) = end_label {
                indent(out, depth);
                let _ = writeln!(out, "{c_label}: ;");
                loop_break_labels.pop();
            }
        }
        Stmt::ForArray {
            label,
            capture,
            ptr_capture,
            idx_capture,
            array,
            ptr_iter: _,
            body,
        } => {
            let arr_ty = env
                .get(array)
                .cloned()
                .ok_or_else(|| format!("unknown array variable {array}"))?;
            // Determine the iterable's length expression, element type, and storage access.
            let (len_expr, elem_ty, via_ptr, via_slice) = match &arr_ty {
                Type::Array { len, elem } => (len.to_string(), elem.as_ref().clone(), false, false),
                Type::Pointer(inner) => match inner.as_ref() {
                    Type::Array { len, elem } => {
                        (len.to_string(), elem.as_ref().clone(), true, false)
                    }
                    _ => {
                        return Err(format!(
                            "for-loop expected array or pointer-to-array, found {arr_ty:?}"
                        ))
                    }
                },
                Type::Slice { elem, .. } => {
                    (format!("({array}).len"), elem.as_ref().clone(), false, true)
                }
                other => {
                    return Err(format!(
                        "for-loop expected array or slice type, found {other:?}"
                    ))
                }
            };
            let cap = capture.as_deref().unwrap_or("_zig_for_x");
            // Use idx_capture as loop variable if provided, else a private name
            let idx_owned;
            let idx: &str = if let Some(ic) = idx_capture {
                ic.as_str()
            } else {
                idx_owned = format!("_{array}_i");
                idx_owned.as_str()
            };
            let end_label = label.as_ref().map(|zig_label| {
                let c_label = format!("zig_lbreak_{}", *temp_id);
                *temp_id += 1;
                loop_break_labels.push((zig_label.clone(), c_label.clone()));
                c_label
            });
            let _ = writeln!(
                out,
                "for (size_t {idx} = 0; {idx} < {len_expr}; {idx}++) {{"
            );

            // Build the capture declaration
            let cap_decl = if *ptr_capture {
                // Pointer capture: `|*elem|` or `|*elem, idx|`
                match &elem_ty {
                    Type::Array { .. } => {
                        let init = if via_slice {
                            format!("({array}).ptr[{idx}]")
                        } else {
                            format!("{array}[{idx}]")
                        };
                        format!("{} = {init}", c_ptr_to_array_elem_decl(cap, &elem_ty))
                    }
                    _ => {
                        // Primitive element; `cell = &array[idx]`
                        if via_slice {
                            format!("{} *{cap} = &({array}).ptr[{idx}]", c_type(&elem_ty))
                        } else if via_ptr {
                            // array is already a pointer; &ptr[idx]
                            format!("{} *{cap} = &{array}[{idx}]", c_type(&elem_ty))
                        } else {
                            format!("{} *{cap} = &{array}[{idx}]", c_type(&elem_ty))
                        }
                    }
                }
            } else {
                // Value capture (original behaviour)
                match &elem_ty {
                    Type::Array { .. } => {
                        let init = if via_slice {
                            format!("({array}).ptr[{idx}]")
                        } else {
                            format!("{array}[{idx}]")
                        };
                        format!("{} = {init}", c_ptr_to_array_elem_decl(cap, &elem_ty))
                    }
                    _ if via_slice => format!("{} {cap} = ({array}).ptr[{idx}]", c_type(&elem_ty)),
                    _ => format!("{} {cap} = {array}[{idx}]", c_type(&elem_ty)),
                }
            };
            let _ = writeln!(out, "    {cap_decl};");

            // Register captures in env
            if let Some(name) = capture {
                let cap_ty = if *ptr_capture {
                    Type::Pointer(Box::new(elem_ty.clone()))
                } else {
                    elem_ty.clone()
                };
                env.insert(name.clone(), cap_ty);
            }
            if let Some(ic) = idx_capture {
                env.insert(ic.clone(), Type::Int(IntType::U64));
            }

            for s in body {
                emit_stmt(
                    out,
                    s,
                    depth + 1,
                    env,
                    return_type,
                    func_returns,
                    temp_id,
                    loop_cont,
                    loop_break_labels,
                )?;
            }
            if let Some(name) = capture {
                env.remove(name);
            }
            if let Some(ic) = idx_capture {
                env.remove(ic);
            }
            indent(out, depth);
            out.push_str("}\n");
            if let Some(c_label) = end_label {
                indent(out, depth);
                let _ = writeln!(out, "{c_label}: ;");
                loop_break_labels.pop();
            }
        }
        Stmt::Switch { scrutinee, arms } => {
            let scrut_s = emit_expr(scrutinee, env, None, func_returns, return_type, temp_id)?;
            let scrutinee_ty = expr_type(scrutinee, env, func_returns);
            match &scrutinee_ty {
                Type::Union(union_name) => {
                    let udef = lookup_union(union_name)
                        .ok_or_else(|| format!("unknown union {union_name}"))?;
                    let mut first = true;
                    for arm in arms {
                        let SwitchTag::UnionVariant {
                            variant, capture, ..
                        } = &arm.tag
                        else {
                            return Err("union switch requires union variant arms".to_string());
                        };
                        let tag = c_union_tag(&udef, variant);
                        if first {
                            let _ = writeln!(out, "if (({scrut_s}).tag == {tag}) {{");
                            first = false;
                        } else {
                            indent(out, depth);
                            let _ = writeln!(out, "}} else if (({scrut_s}).tag == {tag}) {{");
                        }
                        let mut arm_env = env.clone();
                        if let Some(cap) = capture {
                            let payload_ty = union_variant_payload_type(&udef, variant)?;
                            let cap_ct = c_type(&payload_ty);
                            let payload = union_payload_access(&scrut_s, variant);
                            let bind = format!("{cap_ct} {cap} = {payload}");
                            indent(out, depth + 1);
                            let _ = writeln!(out, "{bind};");
                            arm_env.insert(cap.clone(), payload_ty);
                        }
                        for s in &arm.body {
                            emit_stmt(
                                out,
                                s,
                                depth + 1,
                                &mut arm_env,
                                return_type,
                                func_returns,
                                temp_id,
                                loop_cont,
                                loop_break_labels,
                            )?;
                        }
                    }
                    if !arms.is_empty() {
                        indent(out, depth);
                        out.push_str("}\n");
                    }
                }
                Type::Enum(enum_name) => {
                    let mut first = true;
                    for arm in arms {
                        let SwitchTag::EnumVariant { variant, .. } = &arm.tag else {
                            return Err("enum switch requires enum variant arms".to_string());
                        };
                        let tag = c_enum_variant(enum_name, variant);
                        if first {
                            let _ = writeln!(out, "if (({scrut_s}) == {tag}) {{");
                            first = false;
                        } else {
                            indent(out, depth);
                            let _ = writeln!(out, "}} else if (({scrut_s}) == {tag}) {{");
                        }
                        for s in &arm.body {
                            emit_stmt(
                                out,
                                s,
                                depth + 1,
                                env,
                                return_type,
                                func_returns,
                                temp_id,
                                loop_cont,
                                loop_break_labels,
                            )?;
                        }
                    }
                    if !arms.is_empty() {
                        indent(out, depth);
                        out.push_str("}\n");
                    }
                }
                _ => {
                    let mut first = true;
                    for arm in arms {
                        let cond = match &arm.tag {
                            SwitchTag::Int(val) => format!("({scrut_s}) == {val}"),
                            SwitchTag::IntRange { lo, hi, .. } => {
                                format!("({scrut_s}) >= {lo} && ({scrut_s}) <= {hi}")
                            }
                            _ => {
                                return Err("integer switch requires integer arms".to_string());
                            }
                        };
                        if first {
                            let _ = writeln!(out, "if ({cond}) {{");
                            first = false;
                        } else {
                            indent(out, depth);
                            let _ = writeln!(out, "}} else if ({cond}) {{");
                        }
                        let mut arm_env = env.clone();
                        if let SwitchTag::IntRange {
                            capture: Some(cap), ..
                        } = &arm.tag
                        {
                            let ct = c_type(&scrutinee_ty);
                            indent(out, depth + 1);
                            let _ = writeln!(out, "{ct} {cap} = ({scrut_s});");
                            arm_env.insert(cap.clone(), scrutinee_ty.clone());
                        }
                        for s in &arm.body {
                            emit_stmt(
                                out,
                                s,
                                depth + 1,
                                &mut arm_env,
                                return_type,
                                func_returns,
                                temp_id,
                                loop_cont,
                                loop_break_labels,
                            )?;
                        }
                    }
                    if !arms.is_empty() {
                        indent(out, depth);
                        out.push_str("}\n");
                    }
                }
            }
        }
    }
    Ok(())
}

fn assign_target_type(target: &AssignTarget, env: &HashMap<String, Type>) -> Type {
    match target {
        AssignTarget::Name(name) => env.get(name).cloned().unwrap_or(Type::Int(IntType::U8)),
        AssignTarget::Index { base, .. } => indexed_lvalue_type(base, env, &HashMap::new()),
        AssignTarget::Field { base, field } => field_expr_type(base, field, env),
        AssignTarget::Deref(inner) => expr_type(inner, env, &HashMap::new())
            .pointee()
            .unwrap_or(Type::Int(IntType::U8)),
    }
}

fn indexed_lvalue_type_with_funcs(
    base: &Expr,
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
) -> Type {
    indexed_lvalue_type(base, env, func_returns)
}

fn emit_assign_target(
    target: &AssignTarget,
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
    fn_return_type: &Type,
    temp_id: &mut usize,
) -> Result<String, String> {
    Ok(match target {
        AssignTarget::Name(name) => name.clone(),
        AssignTarget::Index { base, index } => {
            format!(
                "{}[{}]",
                emit_expr(base, env, None, func_returns, fn_return_type, temp_id)?,
                emit_expr(
                    index,
                    env,
                    Some(&Type::Int(IntType::U32)),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?
            )
        }
        AssignTarget::Field { base, field } => {
            emit_field_access(base, field, env, func_returns, fn_return_type, temp_id)?
        }
        AssignTarget::Deref(inner) => {
            format!(
                "*{}",
                emit_expr(inner, env, None, func_returns, fn_return_type, temp_id)?
            )
        }
    })
}

fn emit_expr(
    expr: &Expr,
    env: &HashMap<String, Type>,
    expected: Option<&Type>,
    func_returns: &HashMap<String, Type>,
    fn_return_type: &Type,
    temp_id: &mut usize,
) -> Result<String, String> {
    Ok(match expr {
        Expr::Int(n) => {
            if let Some(Type::Optional(inner)) = expected {
                let opt = c_optional_name(inner);
                let ct = c_type(inner);
                format!("({opt}){{ .is_null = false, .value = ({ct})({n}) }}")
            } else if let Some(Type::ErrorUnion { err_set, payload }) = expected {
                let st = c_error_union_name(err_set, payload);
                let ok = c_error_ok_tag(err_set);
                let ct = c_type(payload);
                format!("({st}){{ .err = {ok}, .value = ({ct})({n}) }}")
            } else if let Some(ty) = expected {
                format!("({})({})", c_type(ty), n)
            } else {
                n.to_string()
            }
        }
        Expr::Null => {
            if let Some(Type::Optional(inner)) = expected {
                let opt = c_optional_name(inner);
                format!("({opt}){{ .is_null = true }}")
            } else {
                return Err("null requires optional type context".to_string());
            }
        }
        Expr::TypeValue(_) => return Err("type value has no runtime representation".to_string()),
        Expr::Bool(v) => {
            if *v {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Expr::StringLit(s) => {
            if let Some(slice_ty @ Type::Slice { .. }) = expected {
                let lit = c_string_literal(s);
                format!(
                    "({}){{ .ptr = ({} const *){}, .len = {} }}",
                    c_slice_name(slice_ty),
                    c_type(&Type::Int(IntType::U8)),
                    lit,
                    s.len()
                )
            } else {
                c_string_literal(s)
            }
        }
        Expr::DebugPrint { format, args } => {
            let call =
                emit_debug_print_call(format, args, env, func_returns, fn_return_type, temp_id)?;
            format!("({call}, 0)")
        }
        Expr::Undefined => return Err("undefined has no runtime value".to_string()),
        Expr::Var(name) => {
            if let Some(Type::Optional(inner)) = expected {
                if !matches!(env.get(name), Some(Type::Optional(_))) {
                    let opt = c_optional_name(inner);
                    let ct = c_type(inner);
                    format!("({opt}){{ .is_null = false, .value = ({ct})({name}) }}")
                } else {
                    name.clone()
                }
            } else {
                name.clone()
            }
        }
        Expr::EnumLiteral { enum_name, variant } => c_enum_variant(enum_name, variant),
        Expr::IntFromEnum(inner) => {
            let inner_ty = expr_type(inner, env, func_returns);
            let backing = inner_ty
                .enum_name()
                .and_then(|n| lookup_enum(n))
                .and_then(|e| e.backing)
                .unwrap_or(IntType::U8);
            let inner_s = emit_expr(
                inner,
                env,
                inner_ty
                    .enum_name()
                    .map(|n| Type::Enum(n.to_string()))
                    .as_ref(),
                func_returns,
                fn_return_type,
                temp_id,
            )?;
            format!("({})({})", c_int_type(backing), inner_s)
        }
        Expr::Call { name, args } => {
            let params = lookup_func_params(name);
            let mut parts = Vec::with_capacity(args.len());
            for (i, a) in args.iter().enumerate() {
                let param_ty = params.as_ref().and_then(|ptypes| ptypes.get(i));
                if i == 0 {
                    if let Some(ref ptypes) = params {
                        let arg_ty = expr_type(a, env, func_returns);
                        match ptypes.first() {
                            Some(Type::Pointer(_)) if !arg_ty.is_pointer() => {
                                let arg_s =
                                    emit_expr(a, env, None, func_returns, fn_return_type, temp_id)?;
                                parts.push(format!("&({arg_s})"));
                                continue;
                            }
                            Some(Type::Struct(_)) if arg_ty.is_pointer() => {
                                let arg_s =
                                    emit_expr(a, env, None, func_returns, fn_return_type, temp_id)?;
                                parts.push(format!("(*({arg_s}))"));
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
                let arg_s = emit_expr(a, env, param_ty, func_returns, fn_return_type, temp_id)?;
                parts.push(arg_s);
            }
            format!("{}({})", c_fn(name), parts.join(", "))
        }
        Expr::BinOp { op, left, right } => {
            if matches!(op, BinOp::LogicalAnd | BinOp::LogicalOr) {
                format!(
                    "({} {} {})",
                    emit_expr(
                        left,
                        env,
                        Some(&Type::Bool),
                        func_returns,
                        fn_return_type,
                        temp_id,
                    )?,
                    c_op(*op),
                    emit_expr(
                        right,
                        env,
                        Some(&Type::Bool),
                        func_returns,
                        fn_return_type,
                        temp_id,
                    )?
                )
            } else {
                if let Some(Type::Optional(inner)) = expected {
                    let opt = c_optional_name(inner);
                    let ct = c_type(inner);
                    let value = format!(
                        "({} {} {})",
                        emit_expr(
                            left,
                            env,
                            Some(inner),
                            func_returns,
                            fn_return_type,
                            temp_id,
                        )?,
                        c_op(*op),
                        emit_expr(
                            right,
                            env,
                            Some(inner),
                            func_returns,
                            fn_return_type,
                            temp_id,
                        )?
                    );
                    return Ok(format!(
                        "({opt}){{ .is_null = false, .value = ({ct})({value}) }}"
                    ));
                }
                let ty = combine_types(
                    expr_type(left, env, func_returns),
                    expr_type(right, env, func_returns),
                );
                let expected_ty = expected.cloned().unwrap_or(ty);
                format!(
                    "({} {} {})",
                    emit_expr(
                        left,
                        env,
                        Some(&expected_ty),
                        func_returns,
                        fn_return_type,
                        temp_id,
                    )?,
                    c_op(*op),
                    emit_expr(
                        right,
                        env,
                        Some(&expected_ty),
                        func_returns,
                        fn_return_type,
                        temp_id,
                    )?
                )
            }
        }
        Expr::If {
            cond,
            then_expr,
            else_expr,
        } => {
            let ty = combine_types(
                expr_type(then_expr, env, func_returns),
                expr_type(else_expr, env, func_returns),
            );
            let expected_ty = expected.cloned().unwrap_or(ty);
            format!(
                "({} ? {} : {})",
                emit_expr(
                    cond,
                    env,
                    Some(&Type::Bool),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?,
                emit_expr(
                    then_expr,
                    env,
                    Some(&expected_ty),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?,
                emit_expr(
                    else_expr,
                    env,
                    Some(&expected_ty),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?
            )
        }
        Expr::Switch {
            scrutinee,
            arms,
            default,
        } => emit_switch(
            scrutinee,
            arms,
            default,
            env,
            func_returns,
            fn_return_type,
            temp_id,
        )?,
        Expr::Try(inner) => {
            let inner_ty = expr_type(inner, env, func_returns);
            let Type::ErrorUnion { err_set, payload } = inner_ty else {
                return Err("try requires error union expression".to_string());
            };
            if !matches!(fn_return_type, Type::ErrorUnion { .. }) {
                return Err("try propagation requires error union return type".to_string());
            }
            let st = c_error_union_name(&err_set, &payload);
            let ok = c_error_ok_tag(&err_set);
            let tmp = next_temp(temp_id);
            let ret_err = match fn_return_type {
                Type::ErrorUnion {
                    err_set: ret_err_set,
                    payload: ret_payload,
                } => {
                    if ret_payload.as_ref() != payload.as_ref() {
                        return Err(
                            "try propagation payload type must match function return payload"
                                .to_string(),
                        );
                    }
                    error_union_error_value(
                        &format!("{tmp}.err"),
                        &err_set,
                        ret_err_set,
                        ret_payload,
                    )?
                }
                _ => unreachable!(),
            };
            let call = emit_expr(inner, env, None, func_returns, fn_return_type, temp_id)?;
            format!(
                "({{ {st} {tmp} = {call}; if ({tmp}.err != {ok}) return {ret_err}; {tmp}.value; }})"
            )
        }
        Expr::Catch {
            expr,
            capture,
            fallback,
        } => {
            let inner_ty = expr_type(expr, env, func_returns);
            let Type::ErrorUnion { err_set, payload } = inner_ty else {
                return Err("catch requires error union expression".to_string());
            };
            let st = c_error_union_name(&err_set, &payload);
            let ok = c_error_ok_tag(&err_set);
            let tmp = next_temp(temp_id);
            let call = emit_expr(expr, env, None, func_returns, fn_return_type, temp_id)?;
            let mut fb_env = env.clone();
            let fb = if let Some(cap) = capture {
                let tag_ty = c_error_tag_enum(&err_set);
                fb_env.insert(cap.clone(), Type::Enum(format!("{err_set}_err")));
                let body = emit_expr(
                    fallback,
                    &fb_env,
                    Some(&payload),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?;
                format!("({{ {tag_ty} {cap} = {tmp}.err; {body}; }})")
            } else {
                emit_expr(
                    fallback,
                    env,
                    Some(&payload),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?
            };
            format!("({{ {st} {tmp} = {call}; {tmp}.err == {ok} ? {tmp}.value : ({fb}); }})")
        }
        Expr::CatchReturn {
            expr,
            capture,
            ret_val,
        } => {
            let inner_ty = expr_type(expr, env, func_returns);
            let Type::ErrorUnion { err_set, payload } = inner_ty else {
                return Err("catch requires error union expression".to_string());
            };
            let st = c_error_union_name(&err_set, &payload);
            let ok = c_error_ok_tag(&err_set);
            let tmp = next_temp(temp_id);
            let call = emit_expr(expr, env, None, func_returns, fn_return_type, temp_id)?;
            let mut ret_env = env.clone();
            let ret = if let Some(cap) = capture {
                let tag_ty = c_error_tag_enum(&err_set);
                ret_env.insert(cap.clone(), Type::Enum(format!("{err_set}_err")));
                let body = emit_expr(
                    ret_val,
                    &ret_env,
                    Some(fn_return_type),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?;
                format!("({{ {tag_ty} {cap} = {tmp}.err; {body}; }})")
            } else {
                emit_expr(
                    ret_val,
                    env,
                    Some(fn_return_type),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?
            };
            format!(
                "({{ {st} {tmp} = {call}; if ({tmp}.err != {ok}) return {ret}; {tmp}.value; }})"
            )
        }
        Expr::ErrorLiteral { err_set, variant } => {
            let payload = expected
                .and_then(|t| t.error_union_payload())
                .or_else(|| fn_return_type.error_union_payload())
                .unwrap_or(Type::Int(IntType::U8));
            let st = c_error_union_name(err_set, &payload);
            let tag = c_error_variant_tag(err_set, variant);
            format!("({st}){{ .err = {tag} }}")
        }
        Expr::IntCast { expr, target } => {
            format!(
                "({})({})",
                c_int_type(*target),
                emit_expr(expr, env, None, func_returns, fn_return_type, temp_id)?
            )
        }
        Expr::BitCast { expr, target } => {
            emit_bitcast(expr, target, env, func_returns, fn_return_type, temp_id)?
        }
        Expr::Mod { left, right } => emit_mod_rem(
            left,
            right,
            env,
            expected,
            true,
            func_returns,
            fn_return_type,
            temp_id,
        )?,
        Expr::Rem { left, right } => emit_mod_rem(
            left,
            right,
            env,
            expected,
            false,
            func_returns,
            fn_return_type,
            temp_id,
        )?,
        Expr::UnaryNeg(operand) => {
            let ty = expected
                .cloned()
                .unwrap_or_else(|| expr_type(operand, env, func_returns));
            format!(
                "(-({}))",
                emit_expr(
                    operand,
                    env,
                    Some(&ty),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?
            )
        }
        Expr::UnaryNot(operand) => {
            format!(
                "(!({}))",
                emit_expr(
                    operand,
                    env,
                    Some(&Type::Bool),
                    func_returns,
                    fn_return_type,
                    temp_id,
                )?
            )
        }
        Expr::ArrayLiteral { elems, annotated } => {
            let elem_ty = expected
                .and_then(|t| match t {
                    Type::Array { elem, .. } => Some(elem.as_ref().clone()),
                    other => Some(other.clone()),
                })
                .or_else(|| annotated.as_ref().map(|(_, ty)| ty.clone()))
                .or_else(|| elems.first().map(|e| expr_type(e, env, func_returns)))
                .unwrap_or(Type::Int(IntType::U8));
            let parts: Result<Vec<_>, _> = elems
                .iter()
                .map(|e| {
                    emit_expr(
                        e,
                        env,
                        Some(&elem_ty),
                        func_returns,
                        fn_return_type,
                        temp_id,
                    )
                })
                .collect();
            format!("{{ {} }}", parts?.join(", "))
        }
        Expr::Index { base, index } => {
            let base_ty = expr_type(base, env, func_returns);
            let base_s = emit_expr(base, env, None, func_returns, fn_return_type, temp_id)?;
            let idx_s = emit_expr(
                index,
                env,
                Some(&Type::Int(IntType::U32)),
                func_returns,
                fn_return_type,
                temp_id,
            )?;
            if base_ty.is_slice() {
                format!("({base_s}).ptr[{idx_s}]")
            } else {
                format!("{base_s}[{idx_s}]")
            }
        }
        Expr::SliceRange { base, start, end } => {
            let base_ty = expr_type(base, env, func_returns);
            let slice_ty = expected
                .filter(|t| matches!(t, Type::Slice { .. }))
                .cloned()
                .or_else(|| match base_ty.clone() {
                    Type::Slice { .. } => Some(base_ty.clone()),
                    Type::Array { elem, .. } => Some(Type::Slice {
                        const_: false,
                        elem,
                    }),
                    _ => None,
                })
                .ok_or_else(|| "slice range requires slice type context".to_string())?;
            let base_s = emit_expr(base, env, None, func_returns, fn_return_type, temp_id)?;
            let start_s = emit_expr(
                start,
                env,
                Some(&Type::Int(IntType::U32)),
                func_returns,
                fn_return_type,
                temp_id,
            )?;
            let end_s = emit_expr(
                end,
                env,
                Some(&Type::Int(IntType::U32)),
                func_returns,
                fn_return_type,
                temp_id,
            )?;
            let ptr = if base_ty.is_slice() {
                format!("({base_s}).ptr + ({start_s})")
            } else {
                format!("({base_s}) + ({start_s})")
            };
            format!(
                "({}){{ .ptr = {ptr}, .len = (size_t)(({}) - ({})) }}",
                c_slice_name(&slice_ty),
                end_s,
                start_s
            )
        }
        Expr::AddrOf(inner) => {
            let inner_ty = expr_type(inner, env, func_returns);
            if let Some(slice_ty @ Type::Slice { .. }) = expected {
                if let Type::Array { len, .. } = inner_ty {
                    let arr_s = emit_expr(inner, env, None, func_returns, fn_return_type, temp_id)?;
                    return Ok(array_to_slice_expr(&arr_s, &len, slice_ty));
                }
            }
            format!(
                "&({})",
                emit_expr(inner, env, None, func_returns, fn_return_type, temp_id)?
            )
        }
        Expr::StructLiteral {
            struct_name,
            fields,
        } => {
            let mut parts = Vec::new();
            for (field, value) in fields {
                parts.push(format!(
                    ".{field} = {}",
                    emit_expr(value, env, None, func_returns, fn_return_type, temp_id)?
                ));
            }
            format!("({struct_name}){{ {} }}", parts.join(", "))
        }

        Expr::UnionLiteral {
            union_name,
            variant,
            value,
        } => {
            let un = union_name
                .as_ref()
                .ok_or_else(|| "union literal missing type".to_string())?;
            let udef = lookup_union(un).ok_or_else(|| format!("unknown union {un}"))?;
            let tag = c_union_tag(&udef, variant);
            let mut init = format!("({un}){{ .tag = {tag}");
            if let Some(val) = value {
                let _ = write!(
                    &mut init,
                    ", .payload.{variant} = {}",
                    emit_expr(val, env, None, func_returns, fn_return_type, temp_id)?
                );
            }
            init.push_str(" }");
            init
        }
        Expr::EmptyInit => return Err("empty init has no runtime value".to_string()),
        Expr::FieldAccess { base, field } => {
            let base_ty = expr_type(base, env, func_returns);
            if field == "len" {
                if let Type::Array { len, .. } = base_ty {
                    return Ok(len.to_string());
                }
            }
            let base_s = emit_expr(base, env, None, func_returns, fn_return_type, temp_id)?;
            if let Type::Union(union_name) = base_ty {
                if let Some(udef) = lookup_union(&union_name) {
                    if udef.variants.iter().any(|v| v.name == *field) {
                        let _payload_ty = union_variant_payload_type(&udef, field)?;
                        return Ok(union_payload_access(&base_s, field));
                    }
                }
            }
            emit_field_access(base, field, env, func_returns, fn_return_type, temp_id)?
        }
        Expr::Deref(inner) => {
            format!(
                "(*{})",
                emit_expr(inner, env, None, func_returns, fn_return_type, temp_id)?
            )
        }
        Expr::OptionalUnwrap(inner) => {
            let inner_ty = expr_type(inner, env, func_returns)
                .optional_inner()
                .ok_or_else(|| "optional unwrap requires optional expression".to_string())?;
            let opt_ty = Type::Optional(Box::new(inner_ty));
            let inner_s = emit_expr(
                inner,
                env,
                Some(&opt_ty),
                func_returns,
                fn_return_type,
                temp_id,
            )?;
            format!("({inner_s}).value")
        }
        Expr::Orelse { left, right } => {
            let inner = expr_type(left, env, func_returns)
                .optional_inner()
                .or_else(|| expected.and_then(|t| t.optional_inner()))
                .unwrap_or(Type::Int(IntType::U8));
            let opt_ty = Type::Optional(Box::new(inner.clone()));
            let left_s = emit_expr(
                left,
                env,
                Some(&opt_ty),
                func_returns,
                fn_return_type,
                temp_id,
            )?;
            let right_s = emit_expr(
                right,
                env,
                Some(&inner),
                func_returns,
                fn_return_type,
                temp_id,
            )?;
            format!("(({left_s}).is_null ? ({right_s}) : (({left_s}).value))")
        }
    })
}

fn next_temp(temp_id: &mut usize) -> String {
    let name = format!("__zig_tmp{}", *temp_id);
    *temp_id += 1;
    name
}

fn emit_mod_rem(
    left: &Expr,
    right: &Expr,
    env: &HashMap<String, Type>,
    expected: Option<&Type>,
    is_mod: bool,
    func_returns: &HashMap<String, Type>,
    fn_return_type: &Type,
    temp_id: &mut usize,
) -> Result<String, String> {
    let ty = combine_types(
        expr_type(left, env, func_returns),
        expr_type(right, env, func_returns),
    );
    let it = expected
        .and_then(|t| t.int_type())
        .or_else(|| ty.int_type())
        .unwrap_or(IntType::U8);
    let ct = c_int_type(it);
    let int_type = Type::Int(it);
    let l = emit_expr(
        left,
        env,
        Some(&int_type),
        func_returns,
        fn_return_type,
        temp_id,
    )?;
    let r = emit_expr(
        right,
        env,
        Some(&int_type),
        func_returns,
        fn_return_type,
        temp_id,
    )?;
    if it.is_signed() {
        if is_mod {
            Ok(format!(
                "({{ {ct} __a = ({l}); {ct} __b = ({r}); {ct} __m = __a % __b; \
                 (__m != 0 && ((__m < 0) != (__b < 0))) ? __m + __b : __m; }})"
            ))
        } else {
            Ok(format!("(({ct})({l}) % ({ct})({r}))"))
        }
    } else {
        Ok(format!("(({ct})({l}) % ({ct})({r}))"))
    }
}

fn packed_struct_backing_int(s: &StructDef) -> IntType {
    let bits: u32 = s
        .fields
        .iter()
        .filter_map(|(_, ty)| packed_field_bits(ty))
        .sum();
    let bytes = (bits + 7) / 8;
    match bytes {
        1 => IntType::U8,
        2 => IntType::U16,
        4 => IntType::U32,
        8 => IntType::U64,
        _ => IntType::U8,
    }
}

fn packed_field_bits(ty: &Type) -> Option<u32> {
    match ty {
        Type::Bool => Some(1),
        Type::Int(it) => Some(it.bits()),
        Type::Enum(name) => Some(
            lookup_enum(name)
                .and_then(|e| e.backing)
                .unwrap_or(IntType::U8)
                .bits(),
        ),
        _ => None,
    }
}

fn packed_field_c_type(ty: &Type) -> String {
    match ty {
        Type::Bool => "unsigned int".to_string(),
        Type::Enum(name) => lookup_enum(name)
            .and_then(|e| e.backing)
            .map(c_int_type)
            .unwrap_or_else(|| "uint8_t".to_string()),
        _ => c_type(ty),
    }
}

fn emit_bitcast(
    expr: &Expr,
    target: &Type,
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
    fn_return_type: &Type,
    temp_id: &mut usize,
) -> Result<String, String> {
    let inner_ty = expr_type(expr, env, func_returns);
    let inner_s = emit_expr(expr, env, None, func_returns, fn_return_type, temp_id)?;

    if let Type::Struct(sname) = target {
        if let Some(sdef) = lookup_struct(sname) {
            if sdef.packed {
                let backing = packed_struct_backing_int(&sdef);
                let ct = c_int_type(backing);
                return Ok(format!(
                    "((union {{ {sname} s; {ct} v; }}){{ .v = ({ct})({inner_s}) }}).s"
                ));
            }
        }
        return Err(format!(
            "@bitCast to non-packed struct {sname} is unsupported"
        ));
    }

    let Type::Int(target_int) = target else {
        return Err("@bitCast target must be an integer or packed struct".to_string());
    };
    let ct = c_int_type(*target_int);

    if let Type::Struct(sname) = &inner_ty {
        if let Some(sdef) = lookup_struct(sname) {
            if sdef.packed {
                return Ok(format!(
                    "((union {{ {sname} s; {ct} v; }}){{ .s = {inner_s} }}).v"
                ));
            }
        }
    }

    Ok(format!("({ct})({inner_s})"))
}

fn emit_switch(
    scrutinee: &Expr,
    arms: &[SwitchArm],
    default: &Option<Box<Expr>>,
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
    fn_return_type: &Type,
    temp_id: &mut usize,
) -> Result<String, String> {
    if arms.is_empty() {
        return Err("switch has no arms".to_string());
    }
    let s = emit_expr(scrutinee, env, None, func_returns, fn_return_type, temp_id)?;
    let scrutinee_ty = expr_type(scrutinee, env, func_returns);
    if let Type::Union(union_name) = scrutinee_ty {
        return emit_union_switch(
            &s,
            &union_name,
            arms,
            env,
            func_returns,
            fn_return_type,
            temp_id,
        );
    }
    let mut result = match default {
        Some(d) => emit_expr(d, env, None, func_returns, fn_return_type, temp_id)?,
        None => emit_expr(
            &arms[arms.len() - 1].expr,
            env,
            None,
            func_returns,
            fn_return_type,
            temp_id,
        )?,
    };
    let arm_iter: Box<dyn Iterator<Item = &SwitchArm>> = match default {
        Some(_) => Box::new(arms.iter().rev()),
        None => Box::new(arms.iter().rev().skip(1)),
    };
    for arm in arm_iter {
        let mut arm_env = env.clone();
        let (cond, range_capture) = match (&scrutinee_ty, &arm.tag) {
            (Type::Enum(enum_name), SwitchTag::EnumVariant { variant, .. }) => {
                let tag = c_enum_variant(enum_name, variant);
                (format!("({s}) == {tag}"), None)
            }
            (_, SwitchTag::Int(val)) => (format!("({s}) == {val}"), None),
            (_, SwitchTag::IntRange { lo, hi, capture }) => {
                if let Some(cap) = capture {
                    arm_env.insert(cap.clone(), scrutinee_ty.clone());
                }
                (format!("({s}) >= {lo} && ({s}) <= {hi}"), capture.clone())
            }
            _ => return Err("switch arm tag does not match scrutinee type".to_string()),
        };
        let inner = emit_expr(
            &arm.expr,
            &arm_env,
            None,
            func_returns,
            fn_return_type,
            temp_id,
        )?;
        let arm_expr = if let Some(cap) = range_capture {
            let ct = c_type(&scrutinee_ty);
            format!("({{ {ct} {cap} = ({s}); ({inner}); }})")
        } else {
            inner
        };
        result = format!("({cond} ? ({arm_expr}) : ({result}))");
    }
    Ok(result)
}

fn emit_union_switch(
    s: &str,
    union_name: &str,
    arms: &[SwitchArm],
    env: &HashMap<String, Type>,
    func_returns: &HashMap<String, Type>,
    fn_return_type: &Type,
    temp_id: &mut usize,
) -> Result<String, String> {
    let result_ty = arms
        .last()
        .map(|a| expr_type(&a.expr, env, func_returns))
        .unwrap_or(Type::Int(IntType::U32));
    let ct = c_type(&result_ty);
    let mut out = String::from("({\n");
    let _ = writeln!(out, "    {ct} _zig_switch_result = 0;");
    for arm in arms {
        let SwitchTag::UnionVariant {
            variant, capture, ..
        } = &arm.tag
        else {
            return Err("union switch requires union variant arms".to_string());
        };
        let udef = lookup_union(union_name).ok_or_else(|| format!("unknown union {union_name}"))?;
        let tag = c_union_tag(&udef, variant);
        let arm_expr = emit_expr(
            &arm.expr,
            env,
            Some(&result_ty),
            func_returns,
            fn_return_type,
            temp_id,
        )?;
        if let Some(cap) = capture {
            let payload_ty = union_variant_payload_type(&udef, variant)?;
            let cap_ct = c_type(&payload_ty);
            let payload = union_payload_access(s, variant);
            let bind = format!("{cap_ct} {cap} = {payload}");
            let _ = writeln!(
                out,
                "    if (({s}).tag == {tag}) {{ {bind}; _zig_switch_result = {arm_expr}; }}"
            );
        } else {
            let _ = writeln!(
                out,
                "    if (({s}).tag == {tag}) {{ _zig_switch_result = {arm_expr}; }}"
            );
        }
    }
    out.push_str("    _zig_switch_result;\n})");
    Ok(out)
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
