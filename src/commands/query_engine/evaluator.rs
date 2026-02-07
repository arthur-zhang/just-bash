use indexmap::IndexMap;
use std::collections::HashSet;
#[allow(unused_imports)]
use std::collections::HashMap;

use super::ast::*;
use super::context::*;
use super::operations::*;
use super::value::Value;

// ---------------------------------------------------------------------------
// Main evaluate function
// ---------------------------------------------------------------------------

pub fn evaluate(value: &Value, ast: &AstNode, ctx: &mut EvalContext) -> Result<Vec<Value>, JqError> {
    if ctx.root.is_none() {
        ctx.root = Some(value.clone());
    }

    match ast {
        AstNode::Identity => Ok(vec![value.clone()]),

        AstNode::Field { name, base } => {
            let bases = if let Some(b) = base {
                evaluate(value, b, ctx)?
            } else {
                vec![value.clone()]
            };
            let mut results = Vec::new();
            for v in &bases {
                match v {
                    Value::Object(obj) => {
                        results.push(obj.get(name).cloned().unwrap_or(Value::Null));
                    }
                    Value::Null => results.push(Value::Null),
                    _ => {
                        return Err(JqError::Type(format!(
                            "Cannot index {} with string \"{}\"",
                            v.type_name(),
                            name
                        )));
                    }
                }
            }
            Ok(results)
        }

        AstNode::Index { base, index } => {
            let bases = if let Some(b) = base {
                evaluate(value, b, ctx)?
            } else {
                vec![value.clone()]
            };
            let mut results = Vec::new();
            for v in &bases {
                let indices = evaluate(v, index, ctx)?;
                for idx in &indices {
                    match idx {
                        Value::Number(n) => {
                            if n.is_nan() {
                                results.push(Value::Null);
                                continue;
                            }
                            let truncated = n.trunc() as i64;
                            if let Value::Array(arr) = v {
                                let i = if truncated < 0 {
                                    (arr.len() as i64 + truncated) as usize
                                } else {
                                    truncated as usize
                                };
                                if i < arr.len() {
                                    results.push(arr[i].clone());
                                } else {
                                    results.push(Value::Null);
                                }
                            } else {
                                results.push(Value::Null);
                            }
                        }
                        Value::String(s) => {
                            if let Value::Object(obj) = v {
                                results.push(obj.get(s).cloned().unwrap_or(Value::Null));
                            } else {
                                results.push(Value::Null);
                            }
                        }
                        _ => results.push(Value::Null),
                    }
                }
            }
            Ok(results)
        }

        AstNode::Slice { base, start, end } => {
            let bases = if let Some(b) = base {
                evaluate(value, b, ctx)?
            } else {
                vec![value.clone()]
            };
            let mut results = Vec::new();
            for v in &bases {
                match v {
                    Value::Null => {
                        results.push(Value::Null);
                    }
                    Value::Array(arr) => {
                        let len = arr.len() as i64;
                        let s_vals = if let Some(s) = start {
                            evaluate(value, s, ctx)?
                        } else {
                            vec![Value::Number(0.0)]
                        };
                        let e_vals = if let Some(e) = end {
                            evaluate(value, e, ctx)?
                        } else {
                            vec![Value::Number(len as f64)]
                        };
                        for sv in &s_vals {
                            for ev in &e_vals {
                                let s_raw = match sv {
                                    Value::Number(n) if n.is_nan() => 0,
                                    Value::Number(n) => n.floor() as i64,
                                    _ => 0,
                                };
                                let e_raw = match ev {
                                    Value::Number(n) if n.is_nan() => len,
                                    Value::Number(n) => n.ceil() as i64,
                                    _ => len,
                                };
                                let s_norm = normalize_index(s_raw, len);
                                let e_norm = normalize_index(e_raw, len);
                                if s_norm < e_norm {
                                    results.push(Value::Array(
                                        arr[s_norm as usize..e_norm as usize].to_vec(),
                                    ));
                                } else {
                                    results.push(Value::Array(vec![]));
                                }
                            }
                        }
                    }
                    Value::String(s) => {
                        let chars: Vec<char> = s.chars().collect();
                        let len = chars.len() as i64;
                        let s_vals = if let Some(st) = start {
                            evaluate(value, st, ctx)?
                        } else {
                            vec![Value::Number(0.0)]
                        };
                        let e_vals = if let Some(e) = end {
                            evaluate(value, e, ctx)?
                        } else {
                            vec![Value::Number(len as f64)]
                        };
                        for sv in &s_vals {
                            for ev in &e_vals {
                                let s_raw = match sv {
                                    Value::Number(n) if n.is_nan() => 0,
                                    Value::Number(n) => n.floor() as i64,
                                    _ => 0,
                                };
                                let e_raw = match ev {
                                    Value::Number(n) if n.is_nan() => len,
                                    Value::Number(n) => n.ceil() as i64,
                                    _ => len,
                                };
                                let s_norm = normalize_index(s_raw, len) as usize;
                                let e_norm = normalize_index(e_raw, len) as usize;
                                if s_norm < e_norm {
                                    results.push(Value::String(
                                        chars[s_norm..e_norm].iter().collect(),
                                    ));
                                } else {
                                    results.push(Value::String(String::new()));
                                }
                            }
                        }
                    }
                    _ => {
                        return Err(JqError::Type(format!(
                            "Cannot slice {}",
                            v.type_name()
                        )));
                    }
                }
            }
            Ok(results)
        }

        AstNode::Iterate { base } => {
            let bases = if let Some(b) = base {
                evaluate(value, b, ctx)?
            } else {
                vec![value.clone()]
            };
            let mut results = Vec::new();
            for v in bases {
                match v {
                    Value::Array(arr) => results.extend(arr),
                    Value::Object(obj) => results.extend(obj.into_values()),
                    _ => {}
                }
            }
            Ok(results)
        }

        AstNode::Pipe { left, right } => {
            let left_results = evaluate(value, left, ctx)?;
            let mut pipe_results = Vec::new();
            for v in &left_results {
                match evaluate(v, right, ctx) {
                    Ok(r) => pipe_results.extend(r),
                    Err(JqError::Break { name, results }) => {
                        let mut combined = pipe_results;
                        combined.extend(results);
                        return Err(JqError::Break {
                            name,
                            results: combined,
                        });
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok(pipe_results)
        }

        AstNode::Comma { left, right } => {
            let mut results = match evaluate(value, left, ctx) {
                Ok(r) => r,
                Err(JqError::Break { name, results }) => {
                    return Err(JqError::Break { name, results });
                }
                Err(e) => return Err(e),
            };
            match evaluate(value, right, ctx) {
                Ok(r) => results.extend(r),
                Err(JqError::Break { name, results: brk_results }) => {
                    results.extend(brk_results);
                    return Err(JqError::Break { name, results });
                }
                Err(e) => return Err(e),
            }
            Ok(results)
        }

        AstNode::Literal { value: lit } => Ok(vec![lit.clone()]),

        AstNode::Array { elements } => {
            if let Some(elems) = elements {
                let vals = evaluate(value, elems, ctx)?;
                Ok(vec![Value::Array(vals)])
            } else {
                Ok(vec![Value::Array(vec![])])
            }
        }

        AstNode::Object { entries } => {
            let mut results: Vec<IndexMap<String, Value>> = vec![IndexMap::new()];
            for entry in entries {
                let ObjectEntry::KeyValue { key, value: val_expr } = entry;
                let keys = match key {
                    ObjectKey::Ident(s) => vec![Value::String(s.clone())],
                    ObjectKey::Expr(expr) => evaluate(value, expr, ctx)?,
                };
                let vals = evaluate(value, val_expr, ctx)?;
                let mut new_results = Vec::new();
                for obj in &results {
                    for k in &keys {
                        match k {
                            Value::String(ks) => {
                                for v in &vals {
                                    let mut new_obj = obj.clone();
                                    new_obj.insert(ks.clone(), v.clone());
                                    new_results.push(new_obj);
                                }
                            }
                            _ => {
                                return Err(JqError::Type(format!(
                                    "Cannot use {} as object key",
                                    k.type_name()
                                )));
                            }
                        }
                    }
                }
                results = new_results;
            }
            Ok(results.into_iter().map(Value::Object).collect())
        }

        AstNode::Paren { expr } => evaluate(value, expr, ctx),

        AstNode::BinaryOp { op, left, right } => eval_binary_op(value, op, left, right, ctx),

        AstNode::UnaryOp { op, operand } => {
            let operands = evaluate(value, operand, ctx)?;
            let mut results = Vec::new();
            for v in &operands {
                match op {
                    UnaryOp::Neg => match v {
                        Value::Number(n) => results.push(Value::Number(-n)),
                        _ => results.push(Value::Null),
                    },
                    UnaryOp::Not => results.push(Value::Bool(!v.is_truthy())),
                }
            }
            Ok(results)
        }

        AstNode::Cond {
            cond,
            then_branch,
            elif_branches,
            else_branch,
        } => {
            let conds = evaluate(value, cond, ctx)?;
            let mut results = Vec::new();
            for c in &conds {
                if c.is_truthy() {
                    results.extend(evaluate(value, then_branch, ctx)?);
                } else {
                    let mut handled = false;
                    for (elif_cond, elif_then) in elif_branches {
                        let elif_conds = evaluate(value, elif_cond, ctx)?;
                        if elif_conds.iter().any(|v| v.is_truthy()) {
                            results.extend(evaluate(value, elif_then, ctx)?);
                            handled = true;
                            break;
                        }
                    }
                    if !handled {
                        if let Some(else_b) = else_branch {
                            results.extend(evaluate(value, else_b, ctx)?);
                        } else {
                            results.push(value.clone());
                        }
                    }
                }
            }
            Ok(results)
        }

        AstNode::Try { body, catch } => match evaluate(value, body, ctx) {
            Ok(r) => Ok(r),
            Err(e) => {
                if let Some(catch_expr) = catch {
                    let error_val = match &e {
                        JqError::Value(v) => v.clone(),
                        JqError::Type(msg) | JqError::Runtime(msg) => {
                            Value::String(msg.clone())
                        }
                        _ => Value::String(format!("{}", e)),
                    };
                    evaluate(&error_val, catch_expr, ctx)
                } else {
                    Ok(vec![])
                }
            }
        },

        AstNode::Optional { expr } => match evaluate(value, expr, ctx) {
            Ok(r) => Ok(r),
            Err(_) => Ok(vec![]),
        },

        AstNode::Call { name, args } => eval_builtin(value, name, args, ctx),

        AstNode::VarRef { name } => {
            if name == "$ENV" {
                let mut map = IndexMap::new();
                for (k, v) in &ctx.env {
                    map.insert(k.clone(), Value::String(v.clone()));
                }
                return Ok(vec![Value::Object(map)]);
            }
            Ok(vec![ctx.vars.get(name).cloned().unwrap_or(Value::Null)])
        }

        AstNode::VarBind {
            name,
            value: val_expr,
            body,
            pattern,
            alternatives,
        } => {
            let vals = evaluate(value, val_expr, ctx)?;
            let mut results = Vec::new();
            for v in &vals {
                let mut bound_ctx = None;
                // Build list of patterns to try
                let mut patterns_to_try: Vec<&DestructurePattern> = Vec::new();
                if let Some(p) = pattern {
                    patterns_to_try.push(p);
                } else {
                    // Simple var binding - create a Var pattern on the fly
                }
                if let Some(alts) = alternatives {
                    for alt in alts {
                        patterns_to_try.push(alt);
                    }
                }

                if !patterns_to_try.is_empty() {
                    for p in &patterns_to_try {
                        if let Some(new_ctx) = bind_pattern(ctx, p, v) {
                            bound_ctx = Some(new_ctx);
                            break;
                        }
                    }
                } else {
                    // Simple variable binding
                    bound_ctx = Some(ctx.with_var(name, v.clone()));
                }

                if let Some(mut new_ctx) = bound_ctx {
                    results.extend(evaluate(value, body, &mut new_ctx)?);
                }
            }
            Ok(results)
        }

        AstNode::Def {
            name,
            params,
            func_body,
            body,
        } => {
            let func_key = format!("{}/{}", name, params.len());
            let closure_funcs = ctx.funcs.clone();
            let func_def = FunctionDef {
                params: params.clone(),
                body: (**func_body).clone(),
                closure: Some(closure_funcs),
            };
            let mut new_ctx = ctx.with_func(&func_key, func_def);
            evaluate(value, body, &mut new_ctx)
        }

        AstNode::StringInterp { parts } => {
            let mut result_str = String::new();
            for part in parts {
                match part {
                    StringPart::Literal(s) => result_str.push_str(s),
                    StringPart::Expr(expr) => {
                        let vals = evaluate(value, expr, ctx)?;
                        for v in &vals {
                            match v {
                                Value::String(s) => result_str.push_str(s),
                                _ => result_str.push_str(&v.to_json_string_compact()),
                            }
                        }
                    }
                }
            }
            Ok(vec![Value::String(result_str)])
        }

        AstNode::UpdateOp {
            op: update_op,
            path,
            value: val_expr,
        } => {
            let result = apply_update(value, path, update_op, val_expr, ctx)?;
            Ok(vec![result])
        }

        AstNode::Reduce {
            expr,
            var_name,
            pattern,
            init,
            update,
        } => {
            let items = evaluate(value, expr, ctx)?;
            let init_vals = evaluate(value, init, ctx)?;
            let mut accumulator = init_vals.into_iter().next().unwrap_or(Value::Null);
            for item in &items {
                let mut new_ctx = if let Some(p) = pattern {
                    match bind_pattern(ctx, p, item) {
                        Some(c) => c,
                        None => continue,
                    }
                } else {
                    ctx.with_var(var_name, item.clone())
                };
                let update_vals = evaluate(&accumulator, update, &mut new_ctx)?;
                accumulator = update_vals.into_iter().next().unwrap_or(Value::Null);
            }
            Ok(vec![accumulator])
        }

        AstNode::Foreach {
            expr,
            var_name,
            pattern,
            init,
            update,
            extract,
        } => {
            let items = evaluate(value, expr, ctx)?;
            let init_vals = evaluate(value, init, ctx)?;
            let mut state = init_vals.into_iter().next().unwrap_or(Value::Null);
            let mut foreach_results = Vec::new();
            for item in &items {
                let mut new_ctx = if let Some(p) = pattern {
                    match bind_pattern(ctx, p, item) {
                        Some(c) => c,
                        None => continue,
                    }
                } else {
                    ctx.with_var(var_name, item.clone())
                };
                let update_vals = evaluate(&state, update, &mut new_ctx)?;
                state = update_vals.into_iter().next().unwrap_or(Value::Null);
                if let Some(ext) = extract {
                    let extracted = evaluate(&state, ext, &mut new_ctx)?;
                    foreach_results.extend(extracted);
                } else {
                    foreach_results.push(state.clone());
                }
            }
            Ok(foreach_results)
        }

        AstNode::Label { name, body } => {
            let mut new_ctx = ctx.clone();
            new_ctx.labels.insert(name.clone());
            match evaluate(value, body, &mut new_ctx) {
                Ok(r) => Ok(r),
                Err(JqError::Break { name: brk_name, results }) if brk_name == *name => {
                    Ok(results)
                }
                Err(e) => Err(e),
            }
        }

        AstNode::Break { name } => Err(JqError::Break {
            name: name.clone(),
            results: vec![],
        }),

        AstNode::Recurse => {
            let mut results = Vec::new();
            recurse_walk(value, &mut results);
            Ok(results)
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn normalize_index(idx: i64, len: i64) -> i64 {
    if idx < 0 {
        (len + idx).max(0)
    } else {
        idx.min(len)
    }
}

fn recurse_walk(value: &Value, results: &mut Vec<Value>) {
    results.push(value.clone());
    match value {
        Value::Array(arr) => {
            for item in arr {
                recurse_walk(item, results);
            }
        }
        Value::Object(obj) => {
            for (_, v) in obj {
                recurse_walk(v, results);
            }
        }
        _ => {}
    }
}

fn bind_pattern(ctx: &EvalContext, pattern: &DestructurePattern, value: &Value) -> Option<EvalContext> {
    match pattern {
        DestructurePattern::Var { name } => Some(ctx.with_var(name, value.clone())),
        DestructurePattern::Array { elements } => {
            if let Value::Array(arr) = value {
                let mut new_ctx = ctx.clone();
                for (i, elem) in elements.iter().enumerate() {
                    let elem_val = arr.get(i).cloned().unwrap_or(Value::Null);
                    new_ctx = bind_pattern(&new_ctx, elem, &elem_val)?;
                }
                Some(new_ctx)
            } else {
                None
            }
        }
        DestructurePattern::Object { fields } => {
            if let Value::Object(obj) = value {
                let mut new_ctx = ctx.clone();
                for field in fields {
                    let key = match &field.key {
                        PatternKey::Ident(s) => s.clone(),
                        PatternKey::Expr(expr) => {
                            let mut tmp_ctx = new_ctx.clone();
                            let key_vals = evaluate(value, expr, &mut tmp_ctx).ok()?;
                            if key_vals.is_empty() {
                                return None;
                            }
                            match &key_vals[0] {
                                Value::String(s) => s.clone(),
                                _ => return None,
                            }
                        }
                    };
                    let field_val = obj.get(&key).cloned().unwrap_or(Value::Null);
                    if let Some(ref kv) = field.key_var {
                        new_ctx = new_ctx.with_var(kv, field_val.clone());
                    }
                    new_ctx = bind_pattern(&new_ctx, &field.pattern, &field_val)?;
                }
                Some(new_ctx)
            } else {
                None
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Binary operations
// ---------------------------------------------------------------------------

fn eval_binary_op(
    value: &Value,
    op: &BinaryOp,
    left: &AstNode,
    right: &AstNode,
    ctx: &mut EvalContext,
) -> Result<Vec<Value>, JqError> {
    // Short-circuit for and/or/alt
    if *op == BinaryOp::And {
        let left_vals = evaluate(value, left, ctx)?;
        let mut results = Vec::new();
        for l in &left_vals {
            if !l.is_truthy() {
                results.push(Value::Bool(false));
            } else {
                let right_vals = evaluate(value, right, ctx)?;
                for r in &right_vals {
                    results.push(Value::Bool(r.is_truthy()));
                }
            }
        }
        return Ok(results);
    }

    if *op == BinaryOp::Or {
        let left_vals = evaluate(value, left, ctx)?;
        let mut results = Vec::new();
        for l in &left_vals {
            if l.is_truthy() {
                results.push(Value::Bool(true));
            } else {
                let right_vals = evaluate(value, right, ctx)?;
                for r in &right_vals {
                    results.push(Value::Bool(r.is_truthy()));
                }
            }
        }
        return Ok(results);
    }

    if *op == BinaryOp::Alt {
        let left_vals = evaluate(value, left, ctx)?;
        let non_null: Vec<Value> = left_vals
            .into_iter()
            .filter(|v| !matches!(v, Value::Null | Value::Bool(false)))
            .collect();
        if !non_null.is_empty() {
            return Ok(non_null);
        }
        return evaluate(value, right, ctx);
    }

    let left_vals = evaluate(value, left, ctx)?;
    let right_vals = evaluate(value, right, ctx)?;

    let mut results = Vec::new();
    for l in &left_vals {
        for r in &right_vals {
            results.push(apply_binary_op(op, l, r)?);
        }
    }
    Ok(results)
}

fn apply_binary_op(op: &BinaryOp, l: &Value, r: &Value) -> Result<Value, JqError> {
    match op {
        BinaryOp::Add => {
            if matches!(l, Value::Null) {
                return Ok(r.clone());
            }
            if matches!(r, Value::Null) {
                return Ok(l.clone());
            }
            match (l, r) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
                (Value::String(a), Value::String(b)) => {
                    Ok(Value::String(format!("{}{}", a, b)))
                }
                (Value::Array(a), Value::Array(b)) => {
                    let mut result = a.clone();
                    result.extend(b.iter().cloned());
                    Ok(Value::Array(result))
                }
                (Value::Object(a), Value::Object(b)) => {
                    let mut result = a.clone();
                    for (k, v) in b {
                        result.insert(k.clone(), v.clone());
                    }
                    Ok(Value::Object(result))
                }
                _ => Ok(Value::Null),
            }
        }
        BinaryOp::Sub => match (l, r) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a - b)),
            (Value::Array(a), Value::Array(b)) => {
                let r_set: HashSet<String> =
                    b.iter().map(|x| x.to_json_string_compact()).collect();
                let result: Vec<Value> = a
                    .iter()
                    .filter(|x| !r_set.contains(&x.to_json_string_compact()))
                    .cloned()
                    .collect();
                Ok(Value::Array(result))
            }
            (Value::String(_), Value::String(_)) => Err(JqError::Type(
                "strings cannot be subtracted".to_string(),
            )),
            _ => Ok(Value::Null),
        },
        BinaryOp::Mul => match (l, r) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a * b)),
            (Value::String(s), Value::Number(n)) => {
                let count = *n as usize;
                Ok(Value::String(s.repeat(count)))
            }
            (Value::Object(_), Value::Object(_)) => Ok(deep_merge(l, r)),
            _ => Ok(Value::Null),
        },
        BinaryOp::Div => match (l, r) {
            (Value::Number(a), Value::Number(b)) => {
                if *b == 0.0 {
                    return Err(JqError::Type(format!(
                        "number ({}) and number ({}) cannot be divided because the divisor is zero",
                        a, b
                    )));
                }
                Ok(Value::Number(a / b))
            }
            (Value::String(a), Value::String(b)) => {
                let parts: Vec<Value> = a.split(b.as_str()).map(|s| Value::String(s.to_string())).collect();
                Ok(Value::Array(parts))
            }
            _ => Ok(Value::Null),
        },
        BinaryOp::Mod => match (l, r) {
            (Value::Number(a), Value::Number(b)) => {
                if *b == 0.0 {
                    return Err(JqError::Type(format!(
                        "number ({}) and number ({}) cannot be divided (remainder) because the divisor is zero",
                        a, b
                    )));
                }
                if a.is_infinite() && !a.is_nan() {
                    if b.is_infinite() && !b.is_nan() {
                        return Ok(Value::Number(if *a < 0.0 && *b > 0.0 { -1.0 } else { 0.0 }));
                    }
                    return Ok(Value::Number(0.0));
                }
                Ok(Value::Number(a % b))
            }
            _ => Ok(Value::Null),
        },
        BinaryOp::Eq => Ok(Value::Bool(deep_equal(l, r))),
        BinaryOp::Ne => Ok(Value::Bool(!deep_equal(l, r))),
        BinaryOp::Lt => Ok(Value::Bool(compare_jq(l, r) == std::cmp::Ordering::Less)),
        BinaryOp::Le => Ok(Value::Bool(compare_jq(l, r) != std::cmp::Ordering::Greater)),
        BinaryOp::Gt => Ok(Value::Bool(compare_jq(l, r) == std::cmp::Ordering::Greater)),
        BinaryOp::Ge => Ok(Value::Bool(compare_jq(l, r) != std::cmp::Ordering::Less)),
        BinaryOp::And | BinaryOp::Or | BinaryOp::Alt => {
            unreachable!("handled above")
        }
    }
}

// ---------------------------------------------------------------------------
// Update operations
// ---------------------------------------------------------------------------

fn apply_update(
    root: &Value,
    path_expr: &AstNode,
    op: &UpdateOp,
    value_expr: &AstNode,
    ctx: &mut EvalContext,
) -> Result<Value, JqError> {
    let transformer = |current: &Value, ctx: &mut EvalContext| -> Result<Value, JqError> {
        match op {
            UpdateOp::PipeUpdate => {
                let results = evaluate(current, value_expr, ctx)?;
                Ok(results.into_iter().next().unwrap_or(Value::Null))
            }
            UpdateOp::Assign => {
                let new_vals = evaluate(root, value_expr, ctx)?;
                Ok(new_vals.into_iter().next().unwrap_or(Value::Null))
            }
            _ => {
                let new_vals = evaluate(root, value_expr, ctx)?;
                let new_val = new_vals.into_iter().next().unwrap_or(Value::Null);
                match op {
                    UpdateOp::AddUpdate => apply_binary_op(&BinaryOp::Add, current, &new_val),
                    UpdateOp::SubUpdate => apply_binary_op(&BinaryOp::Sub, current, &new_val),
                    UpdateOp::MulUpdate => apply_binary_op(&BinaryOp::Mul, current, &new_val),
                    UpdateOp::DivUpdate => apply_binary_op(&BinaryOp::Div, current, &new_val),
                    UpdateOp::ModUpdate => apply_binary_op(&BinaryOp::Mod, current, &new_val),
                    UpdateOp::AltUpdate => {
                        if matches!(current, Value::Null | Value::Bool(false)) {
                            Ok(new_val)
                        } else {
                            Ok(current.clone())
                        }
                    }
                    _ => Ok(new_val),
                }
            }
        }
    };

    update_recursive(root, path_expr, ctx, &transformer)
}

fn update_recursive(
    val: &Value,
    path: &AstNode,
    ctx: &mut EvalContext,
    transform: &dyn Fn(&Value, &mut EvalContext) -> Result<Value, JqError>,
) -> Result<Value, JqError> {
    match path {
        AstNode::Identity => transform(val, ctx),

        AstNode::Field { name, base } => {
            if let Some(base_expr) = base {
                update_recursive(val, base_expr, ctx, &|base_val, ctx| {
                    if let Value::Object(obj) = base_val {
                        let mut new_obj = obj.clone();
                        let current = new_obj.get(name).cloned().unwrap_or(Value::Null);
                        let updated = transform(&current, ctx)?;
                        new_obj.insert(name.clone(), updated);
                        Ok(Value::Object(new_obj))
                    } else {
                        Ok(base_val.clone())
                    }
                })
            } else if let Value::Object(obj) = val {
                let mut new_obj = obj.clone();
                let current = new_obj.get(name).cloned().unwrap_or(Value::Null);
                let updated = transform(&current, ctx)?;
                new_obj.insert(name.clone(), updated);
                Ok(Value::Object(new_obj))
            } else {
                Ok(val.clone())
            }
        }

        AstNode::Index { base, index } => {
            let root_clone = val.clone();
            let indices = evaluate(&root_clone, index, ctx)?;
            let idx = indices.into_iter().next().unwrap_or(Value::Null);

            if let Some(base_expr) = base {
                update_recursive(val, base_expr, ctx, &|base_val, ctx| {
                    update_at_index(base_val, &idx, ctx, transform)
                })
            } else {
                update_at_index(val, &idx, ctx, transform)
            }
        }

        AstNode::Iterate { base } => {
            let apply_to_container =
                |container: &Value, ctx: &mut EvalContext| -> Result<Value, JqError> {
                    match container {
                        Value::Array(arr) => {
                            let mut new_arr = Vec::new();
                            for item in arr {
                                new_arr.push(transform(item, ctx)?);
                            }
                            Ok(Value::Array(new_arr))
                        }
                        Value::Object(obj) => {
                            let mut new_obj = IndexMap::new();
                            for (k, v) in obj {
                                new_obj.insert(k.clone(), transform(v, ctx)?);
                            }
                            Ok(Value::Object(new_obj))
                        }
                        _ => Ok(container.clone()),
                    }
                };

            if let Some(base_expr) = base {
                update_recursive(val, base_expr, ctx, &apply_to_container)
            } else {
                apply_to_container(val, ctx)
            }
        }

        AstNode::Pipe { left, right } => {
            update_recursive(val, left, ctx, &|left_val, ctx| {
                update_recursive(left_val, right, ctx, transform)
            })
        }

        _ => transform(val, ctx),
    }
}

fn update_at_index(
    val: &Value,
    idx: &Value,
    ctx: &mut EvalContext,
    transform: &dyn Fn(&Value, &mut EvalContext) -> Result<Value, JqError>,
) -> Result<Value, JqError> {
    match idx {
        Value::Number(n) => {
            let i = n.trunc() as i64;
            match val {
                Value::Array(arr) => {
                    let mut new_arr = arr.clone();
                    let actual_i = if i < 0 {
                        (new_arr.len() as i64 + i) as usize
                    } else {
                        i as usize
                    };
                    while new_arr.len() <= actual_i {
                        new_arr.push(Value::Null);
                    }
                    let current = new_arr[actual_i].clone();
                    new_arr[actual_i] = transform(&current, ctx)?;
                    Ok(Value::Array(new_arr))
                }
                Value::Null => {
                    let mut new_arr = Vec::new();
                    let actual_i = i as usize;
                    while new_arr.len() <= actual_i {
                        new_arr.push(Value::Null);
                    }
                    new_arr[actual_i] = transform(&Value::Null, ctx)?;
                    Ok(Value::Array(new_arr))
                }
                _ => Ok(val.clone()),
            }
        }
        Value::String(s) => {
            if let Value::Object(obj) = val {
                let mut new_obj = obj.clone();
                let current = new_obj.get(s).cloned().unwrap_or(Value::Null);
                let updated = transform(&current, ctx)?;
                new_obj.insert(s.clone(), updated);
                Ok(Value::Object(new_obj))
            } else {
                Ok(val.clone())
            }
        }
        _ => Ok(val.clone()),
    }
}

// ---------------------------------------------------------------------------
// Delete operation
// ---------------------------------------------------------------------------

fn apply_del(root: &Value, path_expr: &AstNode, ctx: &mut EvalContext) -> Result<Value, JqError> {
    match path_expr {
        AstNode::Identity => Ok(Value::Null),

        AstNode::Field { name, base } => {
            if let Some(base_expr) = base {
                let nested_vals = evaluate(root, base_expr, ctx)?;
                let nested = nested_vals.into_iter().next().unwrap_or(Value::Null);
                let simple_field = AstNode::Field {
                    name: name.clone(),
                    base: None,
                };
                let modified = apply_del(&nested, &simple_field, ctx)?;
                set_at_path(root, base_expr, &modified, ctx)
            } else if let Value::Object(obj) = root {
                let mut new_obj = obj.clone();
                new_obj.shift_remove(name);
                Ok(Value::Object(new_obj))
            } else {
                Ok(root.clone())
            }
        }

        AstNode::Index { base, index } => {
            if let Some(base_expr) = base {
                let nested_vals = evaluate(root, base_expr, ctx)?;
                let nested = nested_vals.into_iter().next().unwrap_or(Value::Null);
                let simple_index = AstNode::Index {
                    base: None,
                    index: index.clone(),
                };
                let modified = apply_del(&nested, &simple_index, ctx)?;
                set_at_path(root, base_expr, &modified, ctx)
            } else {
                let root_clone = root.clone();
                let indices = evaluate(&root_clone, index, ctx)?;
                let idx = indices.into_iter().next().unwrap_or(Value::Null);
                match (&idx, root) {
                    (Value::Number(n), Value::Array(arr)) => {
                        let i = n.trunc() as i64;
                        let actual_i = if i < 0 {
                            (arr.len() as i64 + i) as usize
                        } else {
                            i as usize
                        };
                        if actual_i < arr.len() {
                            let mut new_arr = arr.clone();
                            new_arr.remove(actual_i);
                            Ok(Value::Array(new_arr))
                        } else {
                            Ok(root.clone())
                        }
                    }
                    (Value::String(s), Value::Object(obj)) => {
                        let mut new_obj = obj.clone();
                        new_obj.shift_remove(s);
                        Ok(Value::Object(new_obj))
                    }
                    _ => Ok(root.clone()),
                }
            }
        }

        AstNode::Iterate { .. } => match root {
            Value::Array(_) => Ok(Value::Array(vec![])),
            Value::Object(_) => Ok(Value::Object(IndexMap::new())),
            _ => Ok(root.clone()),
        },

        AstNode::Pipe { left, right } => {
            let nested_vals = evaluate(root, left, ctx)?;
            let nested = nested_vals.into_iter().next().unwrap_or(Value::Null);
            let modified = apply_del(&nested, right, ctx)?;
            set_at_path(root, left, &modified, ctx)
        }

        _ => Ok(root.clone()),
    }
}

fn set_at_path(
    obj: &Value,
    path_node: &AstNode,
    new_val: &Value,
    ctx: &mut EvalContext,
) -> Result<Value, JqError> {
    match path_node {
        AstNode::Identity => Ok(new_val.clone()),
        AstNode::Field { name, base: None } => {
            if let Value::Object(o) = obj {
                let mut new_obj = o.clone();
                new_obj.insert(name.clone(), new_val.clone());
                Ok(Value::Object(new_obj))
            } else {
                Ok(obj.clone())
            }
        }
        AstNode::Field { name, base: Some(base_expr) } => {
            let nested_vals = evaluate(obj, base_expr, ctx)?;
            let nested = nested_vals.into_iter().next().unwrap_or(Value::Null);
            let simple_field = AstNode::Field {
                name: name.clone(),
                base: None,
            };
            let modified = set_at_path(&nested, &simple_field, new_val, ctx)?;
            set_at_path(obj, base_expr, &modified, ctx)
        }
        AstNode::Index { base: None, index } => {
            let obj_clone = obj.clone();
            let indices = evaluate(&obj_clone, index, ctx)?;
            let idx = indices.into_iter().next().unwrap_or(Value::Null);
            match (&idx, obj) {
                (Value::Number(n), Value::Array(arr)) => {
                    let i = n.trunc() as i64;
                    let actual_i = if i < 0 {
                        (arr.len() as i64 + i) as usize
                    } else {
                        i as usize
                    };
                    if actual_i < arr.len() {
                        let mut new_arr = arr.clone();
                        new_arr[actual_i] = new_val.clone();
                        Ok(Value::Array(new_arr))
                    } else {
                        Ok(obj.clone())
                    }
                }
                (Value::String(s), Value::Object(o)) => {
                    let mut new_obj = o.clone();
                    new_obj.insert(s.clone(), new_val.clone());
                    Ok(Value::Object(new_obj))
                }
                _ => Ok(obj.clone()),
            }
        }
        AstNode::Pipe { left, right } => {
            let inner_vals = evaluate(obj, left, ctx)?;
            let inner = inner_vals.into_iter().next().unwrap_or(Value::Null);
            let modified = set_at_path(&inner, right, new_val, ctx)?;
            set_at_path(obj, left, &modified, ctx)
        }
        _ => Ok(obj.clone()),
    }
}

// ---------------------------------------------------------------------------
// Builtin dispatch
// ---------------------------------------------------------------------------

fn eval_builtin(
    value: &Value,
    name: &str,
    args: &[AstNode],
    ctx: &mut EvalContext,
) -> Result<Vec<Value>, JqError> {
    // Simple math functions (single numeric input, no args)
    if let Some(result) = eval_simple_math(value, name) {
        return Ok(vec![result]);
    }

    match name {
        // ===== Type builtins =====
        "type" => Ok(vec![Value::String(value.type_name().to_string())]),
        "infinite" => Ok(vec![Value::Number(f64::INFINITY)]),
        "nan" => Ok(vec![Value::Number(f64::NAN)]),
        "isinfinite" => Ok(vec![Value::Bool(
            matches!(value, Value::Number(n) if n.is_infinite()),
        )]),
        "isnan" => Ok(vec![Value::Bool(
            matches!(value, Value::Number(n) if n.is_nan()),
        )]),
        "isnormal" => Ok(vec![Value::Bool(
            matches!(value, Value::Number(n) if n.is_finite() && *n != 0.0),
        )]),
        "isfinite" => Ok(vec![Value::Bool(
            matches!(value, Value::Number(n) if n.is_finite()),
        )]),
        "numbers" => {
            if matches!(value, Value::Number(_)) {
                Ok(vec![value.clone()])
            } else {
                Ok(vec![])
            }
        }
        "strings" => {
            if matches!(value, Value::String(_)) {
                Ok(vec![value.clone()])
            } else {
                Ok(vec![])
            }
        }
        "booleans" => {
            if matches!(value, Value::Bool(_)) {
                Ok(vec![value.clone()])
            } else {
                Ok(vec![])
            }
        }
        "nulls" => {
            if matches!(value, Value::Null) {
                Ok(vec![value.clone()])
            } else {
                Ok(vec![])
            }
        }
        "arrays" => {
            if matches!(value, Value::Array(_)) {
                Ok(vec![value.clone()])
            } else {
                Ok(vec![])
            }
        }
        "objects" => {
            if matches!(value, Value::Object(_)) {
                Ok(vec![value.clone()])
            } else {
                Ok(vec![])
            }
        }
        "iterables" => {
            if matches!(value, Value::Array(_) | Value::Object(_)) {
                Ok(vec![value.clone()])
            } else {
                Ok(vec![])
            }
        }
        "scalars" => {
            if !matches!(value, Value::Array(_) | Value::Object(_)) {
                Ok(vec![value.clone()])
            } else {
                Ok(vec![])
            }
        }
        "values" => {
            if matches!(value, Value::Null) {
                Ok(vec![])
            } else {
                Ok(vec![value.clone()])
            }
        }
        "not" => Ok(vec![Value::Bool(!value.is_truthy())]),
        "null" => Ok(vec![Value::Null]),
        "true" => Ok(vec![Value::Bool(true)]),
        "false" => Ok(vec![Value::Bool(false)]),
        "empty" => Ok(vec![]),

        // ===== Object builtins =====
        "length" => match value {
            Value::String(s) => Ok(vec![Value::Number(s.len() as f64)]),
            Value::Array(a) => Ok(vec![Value::Number(a.len() as f64)]),
            Value::Object(o) => Ok(vec![Value::Number(o.len() as f64)]),
            Value::Null => Ok(vec![Value::Number(0.0)]),
            Value::Number(n) => Ok(vec![Value::Number(n.abs())]),
            _ => Ok(vec![Value::Null]),
        },

        "utf8bytelength" => match value {
            Value::String(s) => Ok(vec![Value::Number(s.len() as f64)]),
            _ => Err(JqError::Type(format!(
                "{} only strings have UTF-8 byte length",
                value.type_name()
            ))),
        },

        "keys" => match value {
            Value::Array(a) => Ok(vec![Value::Array(
                (0..a.len()).map(|i| Value::Number(i as f64)).collect(),
            )]),
            Value::Object(o) => {
                let mut keys: Vec<String> = o.keys().cloned().collect();
                keys.sort();
                Ok(vec![Value::Array(
                    keys.into_iter().map(Value::String).collect(),
                )])
            }
            _ => Ok(vec![Value::Null]),
        },

        "keys_unsorted" => match value {
            Value::Array(a) => Ok(vec![Value::Array(
                (0..a.len()).map(|i| Value::Number(i as f64)).collect(),
            )]),
            Value::Object(o) => Ok(vec![Value::Array(
                o.keys().cloned().map(Value::String).collect(),
            )]),
            _ => Ok(vec![Value::Null]),
        },

        "to_entries" => {
            if let Value::Object(obj) = value {
                let entries: Vec<Value> = obj
                    .iter()
                    .map(|(k, v)| {
                        let mut entry = IndexMap::new();
                        entry.insert("key".to_string(), Value::String(k.clone()));
                        entry.insert("value".to_string(), v.clone());
                        Value::Object(entry)
                    })
                    .collect();
                Ok(vec![Value::Array(entries)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "from_entries" => {
            if let Value::Array(arr) = value {
                let mut result = IndexMap::new();
                for item in arr {
                    if let Value::Object(obj) = item {
                        let key = obj
                            .get("key")
                            .or_else(|| obj.get("Key"))
                            .or_else(|| obj.get("name"))
                            .or_else(|| obj.get("Name"))
                            .or_else(|| obj.get("k"));
                        let val = obj
                            .get("value")
                            .or_else(|| obj.get("Value"))
                            .or_else(|| obj.get("v"));
                        if let Some(k) = key {
                            let key_str = match k {
                                Value::String(s) => s.clone(),
                                _ => format!("{}", k),
                            };
                            result.insert(
                                key_str,
                                val.cloned().unwrap_or(Value::Null),
                            );
                        }
                    }
                }
                Ok(vec![Value::Object(result)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "with_entries" => {
            if args.is_empty() {
                return Ok(vec![value.clone()]);
            }
            if let Value::Object(obj) = value {
                let entries: Vec<Value> = obj
                    .iter()
                    .map(|(k, v)| {
                        let mut entry = IndexMap::new();
                        entry.insert("key".to_string(), Value::String(k.clone()));
                        entry.insert("value".to_string(), v.clone());
                        Value::Object(entry)
                    })
                    .collect();
                let mut mapped = Vec::new();
                for e in &entries {
                    mapped.extend(evaluate(e, &args[0], ctx)?);
                }
                let mut result = IndexMap::new();
                for item in &mapped {
                    if let Value::Object(obj) = item {
                        let key = obj
                            .get("key")
                            .or_else(|| obj.get("name"))
                            .or_else(|| obj.get("k"));
                        let val = obj
                            .get("value")
                            .or_else(|| obj.get("v"));
                        if let Some(k) = key {
                            let key_str = match k {
                                Value::String(s) => s.clone(),
                                _ => format!("{}", k),
                            };
                            result.insert(
                                key_str,
                                val.cloned().unwrap_or(Value::Null),
                            );
                        }
                    }
                }
                Ok(vec![Value::Object(result)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "reverse" => match value {
            Value::Array(a) => {
                let mut r = a.clone();
                r.reverse();
                Ok(vec![Value::Array(r)])
            }
            Value::String(s) => Ok(vec![Value::String(
                s.chars().rev().collect(),
            )]),
            _ => Ok(vec![Value::Null]),
        },

        "flatten" => {
            if let Value::Array(arr) = value {
                let depth = if !args.is_empty() {
                    let d = evaluate(value, &args[0], ctx)?;
                    match d.first() {
                        Some(Value::Number(n)) => *n as i64,
                        _ => i64::MAX,
                    }
                } else {
                    i64::MAX
                };
                if depth < 0 {
                    return Err(JqError::Runtime(
                        "flatten depth must not be negative".to_string(),
                    ));
                }
                Ok(vec![Value::Array(flatten_array(arr, depth))])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "unique" => {
            if let Value::Array(arr) = value {
                let mut seen = HashSet::new();
                let mut result = Vec::new();
                for item in arr {
                    let key = item.to_json_string_compact();
                    if seen.insert(key) {
                        result.push(item.clone());
                    }
                }
                Ok(vec![Value::Array(result)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "tojson" => Ok(vec![Value::String(value.to_json_string_compact())]),

        "fromjson" => {
            if let Value::String(s) = value {
                let trimmed = s.trim().to_lowercase();
                if trimmed == "nan" {
                    return Ok(vec![Value::Number(f64::NAN)]);
                }
                if trimmed == "inf" || trimmed == "infinity" {
                    return Ok(vec![Value::Number(f64::INFINITY)]);
                }
                if trimmed == "-inf" || trimmed == "-infinity" {
                    return Ok(vec![Value::Number(f64::NEG_INFINITY)]);
                }
                match serde_json::from_str::<serde_json::Value>(s) {
                    Ok(v) => Ok(vec![Value::from_serde_json(v)]),
                    Err(e) => Err(JqError::Runtime(format!("Invalid JSON: {}", e))),
                }
            } else {
                Ok(vec![value.clone()])
            }
        }

        "tostring" => match value {
            Value::String(_) => Ok(vec![value.clone()]),
            _ => Ok(vec![Value::String(value.to_json_string_compact())]),
        },

        "tonumber" => match value {
            Value::Number(_) => Ok(vec![value.clone()]),
            Value::String(s) => match s.parse::<f64>() {
                Ok(n) => Ok(vec![Value::Number(n)]),
                Err(_) => Err(JqError::Type(format!(
                    "{} cannot be parsed as a number",
                    s
                ))),
            },
            _ => Err(JqError::Type(format!(
                "{} cannot be parsed as a number",
                value.type_name()
            ))),
        },

        "toboolean" => match value {
            Value::Bool(_) => Ok(vec![value.clone()]),
            Value::String(s) => match s.as_str() {
                "true" => Ok(vec![Value::Bool(true)]),
                "false" => Ok(vec![Value::Bool(false)]),
                _ => Err(JqError::Type(format!(
                    "string ({}) cannot be parsed as a boolean",
                    s
                ))),
            },
            _ => Err(JqError::Type(format!(
                "{} cannot be parsed as a boolean",
                value.type_name()
            ))),
        },

        // ===== Array builtins =====
        "sort" => {
            if let Value::Array(arr) = value {
                let mut sorted = arr.clone();
                sorted.sort_by(|a, b| compare_jq(a, b));
                Ok(vec![Value::Array(sorted)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "sort_by" => {
            if let Value::Array(arr) = value {
                if args.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let mut items: Vec<(Value, Value)> = Vec::new();
                for item in arr {
                    let key = evaluate(item, &args[0], ctx)?
                        .into_iter()
                        .next()
                        .unwrap_or(Value::Null);
                    items.push((item.clone(), key));
                }
                items.sort_by(|a, b| compare_jq(&a.1, &b.1));
                Ok(vec![Value::Array(
                    items.into_iter().map(|(v, _)| v).collect(),
                )])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "group_by" => {
            if let Value::Array(arr) = value {
                if args.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let mut groups: IndexMap<String, Vec<Value>> = IndexMap::new();
                for item in arr {
                    let key = evaluate(item, &args[0], ctx)?
                        .into_iter()
                        .next()
                        .unwrap_or(Value::Null);
                    let key_str = key.to_json_string_compact();
                    groups.entry(key_str).or_default().push(item.clone());
                }
                Ok(vec![Value::Array(
                    groups.into_values().map(Value::Array).collect(),
                )])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "unique_by" => {
            if let Value::Array(arr) = value {
                if args.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let mut seen = IndexMap::new();
                for item in arr {
                    let key = evaluate(item, &args[0], ctx)?
                        .into_iter()
                        .next()
                        .unwrap_or(Value::Null);
                    let key_str = key.to_json_string_compact();
                    seen.entry(key_str).or_insert((item.clone(), key));
                }
                let mut entries: Vec<(Value, Value)> = seen.into_values().collect();
                entries.sort_by(|a, b| compare_jq(&a.1, &b.1));
                Ok(vec![Value::Array(
                    entries.into_iter().map(|(v, _)| v).collect(),
                )])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "max" => {
            if let Value::Array(arr) = value {
                if arr.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let result = arr
                    .iter()
                    .max_by(|a, b| compare_jq(a, b))
                    .cloned()
                    .unwrap_or(Value::Null);
                Ok(vec![result])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "max_by" => {
            if let Value::Array(arr) = value {
                if arr.is_empty() || args.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let mut best = &arr[0];
                let mut best_key = evaluate(best, &args[0], ctx)?
                    .into_iter()
                    .next()
                    .unwrap_or(Value::Null);
                for item in arr.iter().skip(1) {
                    let key = evaluate(item, &args[0], ctx)?
                        .into_iter()
                        .next()
                        .unwrap_or(Value::Null);
                    if compare_jq(&key, &best_key) == std::cmp::Ordering::Greater {
                        best = item;
                        best_key = key;
                    }
                }
                Ok(vec![best.clone()])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "min" => {
            if let Value::Array(arr) = value {
                if arr.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let result = arr
                    .iter()
                    .min_by(|a, b| compare_jq(a, b))
                    .cloned()
                    .unwrap_or(Value::Null);
                Ok(vec![result])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "min_by" => {
            if let Value::Array(arr) = value {
                if arr.is_empty() || args.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let mut best = &arr[0];
                let mut best_key = evaluate(best, &args[0], ctx)?
                    .into_iter()
                    .next()
                    .unwrap_or(Value::Null);
                for item in arr.iter().skip(1) {
                    let key = evaluate(item, &args[0], ctx)?
                        .into_iter()
                        .next()
                        .unwrap_or(Value::Null);
                    if compare_jq(&key, &best_key) == std::cmp::Ordering::Less {
                        best = item;
                        best_key = key;
                    }
                }
                Ok(vec![best.clone()])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "add" => {
            if !args.is_empty() {
                let collected = evaluate(value, &args[0], ctx)?;
                return Ok(vec![add_values(&collected)]);
            }
            if let Value::Array(arr) = value {
                Ok(vec![add_values(arr)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "any" => {
            if args.len() >= 2 {
                let gen_values = evaluate(value, &args[0], ctx).unwrap_or_default();
                for v in &gen_values {
                    let cond = evaluate(v, &args[1], ctx)?;
                    if cond.iter().any(|c| c.is_truthy()) {
                        return Ok(vec![Value::Bool(true)]);
                    }
                }
                return Ok(vec![Value::Bool(false)]);
            }
            if args.len() == 1 {
                if let Value::Array(arr) = value {
                    let result = arr
                        .iter()
                        .any(|item| {
                            evaluate(item, &args[0], ctx)
                                .ok()
                                .and_then(|r| r.first().cloned())
                                .map(|v| v.is_truthy())
                                .unwrap_or(false)
                        });
                    return Ok(vec![Value::Bool(result)]);
                }
                return Ok(vec![Value::Bool(false)]);
            }
            if let Value::Array(arr) = value {
                Ok(vec![Value::Bool(arr.iter().any(|v| v.is_truthy()))])
            } else {
                Ok(vec![Value::Bool(false)])
            }
        }

        "all" => {
            if args.len() >= 2 {
                let gen_values = evaluate(value, &args[0], ctx).unwrap_or_default();
                for v in &gen_values {
                    let cond = evaluate(v, &args[1], ctx)?;
                    if !cond.iter().any(|c| c.is_truthy()) {
                        return Ok(vec![Value::Bool(false)]);
                    }
                }
                return Ok(vec![Value::Bool(true)]);
            }
            if args.len() == 1 {
                if let Value::Array(arr) = value {
                    let result = arr
                        .iter()
                        .all(|item| {
                            evaluate(item, &args[0], ctx)
                                .ok()
                                .and_then(|r| r.first().cloned())
                                .map(|v| v.is_truthy())
                                .unwrap_or(false)
                        });
                    return Ok(vec![Value::Bool(result)]);
                }
                return Ok(vec![Value::Bool(true)]);
            }
            if let Value::Array(arr) = value {
                Ok(vec![Value::Bool(arr.iter().all(|v| v.is_truthy()))])
            } else {
                Ok(vec![Value::Bool(true)])
            }
        }

        "select" => {
            if args.is_empty() {
                return Ok(vec![value.clone()]);
            }
            let conds = evaluate(value, &args[0], ctx)?;
            if conds.iter().any(|c| c.is_truthy()) {
                Ok(vec![value.clone()])
            } else {
                Ok(vec![])
            }
        }

        "map" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            if let Value::Array(arr) = value {
                let mut results = Vec::new();
                for item in arr {
                    results.extend(evaluate(item, &args[0], ctx)?);
                }
                Ok(vec![Value::Array(results)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "map_values" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            match value {
                Value::Array(arr) => {
                    let mut results = Vec::new();
                    for item in arr {
                        results.extend(evaluate(item, &args[0], ctx)?);
                    }
                    Ok(vec![Value::Array(results)])
                }
                Value::Object(obj) => {
                    let mut result = IndexMap::new();
                    for (k, v) in obj {
                        let mapped = evaluate(v, &args[0], ctx)?;
                        if let Some(first) = mapped.into_iter().next() {
                            result.insert(k.clone(), first);
                        }
                    }
                    Ok(vec![Value::Object(result)])
                }
                _ => Ok(vec![Value::Null]),
            }
        }

        "has" => {
            if args.is_empty() {
                return Ok(vec![Value::Bool(false)]);
            }
            let keys = evaluate(value, &args[0], ctx)?;
            let key = keys.first().cloned().unwrap_or(Value::Null);
            match (value, &key) {
                (Value::Array(arr), Value::Number(n)) => {
                    let i = *n as i64;
                    Ok(vec![Value::Bool(i >= 0 && (i as usize) < arr.len())])
                }
                (Value::Object(obj), Value::String(s)) => {
                    Ok(vec![Value::Bool(obj.contains_key(s))])
                }
                _ => Ok(vec![Value::Bool(false)]),
            }
        }

        "in" => {
            if args.is_empty() {
                return Ok(vec![Value::Bool(false)]);
            }
            let objs = evaluate(value, &args[0], ctx)?;
            let obj = objs.first().cloned().unwrap_or(Value::Null);
            match (&obj, value) {
                (Value::Array(arr), Value::Number(n)) => {
                    let i = *n as i64;
                    Ok(vec![Value::Bool(i >= 0 && (i as usize) < arr.len())])
                }
                (Value::Object(obj), Value::String(s)) => {
                    Ok(vec![Value::Bool(obj.contains_key(s))])
                }
                _ => Ok(vec![Value::Bool(false)]),
            }
        }

        "contains" => {
            if args.is_empty() {
                return Ok(vec![Value::Bool(false)]);
            }
            let others = evaluate(value, &args[0], ctx)?;
            let other = others.first().cloned().unwrap_or(Value::Null);
            Ok(vec![Value::Bool(contains_deep(value, &other))])
        }

        "inside" => {
            if args.is_empty() {
                return Ok(vec![Value::Bool(false)]);
            }
            let others = evaluate(value, &args[0], ctx)?;
            let other = others.first().cloned().unwrap_or(Value::Null);
            Ok(vec![Value::Bool(contains_deep(&other, value))])
        }

        "bsearch" => {
            if let Value::Array(arr) = value {
                if args.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let targets = evaluate(value, &args[0], ctx)?;
                let mut results = Vec::new();
                for target in &targets {
                    let mut lo: usize = 0;
                    let mut hi: usize = arr.len();
                    while lo < hi {
                        let mid = (lo + hi) / 2;
                        if compare_jq(&arr[mid], target) == std::cmp::Ordering::Less {
                            lo = mid + 1;
                        } else {
                            hi = mid;
                        }
                    }
                    if lo < arr.len() && compare_jq(&arr[lo], target) == std::cmp::Ordering::Equal
                    {
                        results.push(Value::Number(lo as f64));
                    } else {
                        results.push(Value::Number(-(lo as f64) - 1.0));
                    }
                }
                Ok(results)
            } else {
                Err(JqError::Type(format!(
                    "{} cannot be searched from",
                    value.type_name()
                )))
            }
        }

        // ===== String builtins =====
        "join" => {
            if let Value::Array(arr) = value {
                for x in arr {
                    if matches!(x, Value::Array(_) | Value::Object(_)) {
                        return Err(JqError::Runtime(
                            "cannot join: contains arrays or objects".to_string(),
                        ));
                    }
                }
                let seps = if !args.is_empty() {
                    evaluate(value, &args[0], ctx)?
                } else {
                    vec![Value::String(String::new())]
                };
                let mut results = Vec::new();
                for sep in &seps {
                    let sep_str = match sep {
                        Value::String(s) => s.clone(),
                        _ => format!("{}", sep),
                    };
                    let joined: String = arr
                        .iter()
                        .map(|x| match x {
                            Value::Null => String::new(),
                            Value::String(s) => s.clone(),
                            _ => format!("{}", x),
                        })
                        .collect::<Vec<_>>()
                        .join(&sep_str);
                    results.push(Value::String(joined));
                }
                Ok(results)
            } else {
                Ok(vec![Value::Null])
            }
        }

        "split" => {
            if let Value::String(s) = value {
                if args.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let seps = evaluate(value, &args[0], ctx)?;
                let sep = match seps.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Ok(vec![Value::Null]),
                };
                let parts: Vec<Value> = s
                    .split(&sep)
                    .map(|p| Value::String(p.to_string()))
                    .collect();
                Ok(vec![Value::Array(parts)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "splits" => {
            if let Value::String(s) = value {
                if args.is_empty() {
                    return Ok(vec![]);
                }
                let patterns = evaluate(value, &args[0], ctx)?;
                let pattern = match patterns.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Ok(vec![]),
                };
                let flags = if args.len() > 1 {
                    match evaluate(value, &args[1], ctx)?.first() {
                        Some(Value::String(s)) => s.clone(),
                        _ => "g".to_string(),
                    }
                } else {
                    "g".to_string()
                };
                match regex_lite::Regex::new(&pattern) {
                    Ok(re) => {
                        let case_insensitive = flags.contains('i');
                        let actual_re = if case_insensitive {
                            regex_lite::Regex::new(&format!("(?i){}", pattern))
                                .unwrap_or(re)
                        } else {
                            re
                        };
                        let parts: Vec<Value> = actual_re
                            .split(s)
                            .map(|p| Value::String(p.to_string()))
                            .collect();
                        Ok(parts)
                    }
                    Err(_) => Ok(vec![]),
                }
            } else {
                Ok(vec![])
            }
        }

        "scan" => {
            if let Value::String(s) = value {
                if args.is_empty() {
                    return Ok(vec![]);
                }
                let patterns = evaluate(value, &args[0], ctx)?;
                let pattern = match patterns.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Ok(vec![]),
                };
                match regex_lite::Regex::new(&pattern) {
                    Ok(re) => {
                        let mut results = Vec::new();
                        for m in re.find_iter(s) {
                            results.push(Value::String(m.as_str().to_string()));
                        }
                        Ok(results)
                    }
                    Err(_) => Ok(vec![]),
                }
            } else {
                Ok(vec![])
            }
        }

        "test" => {
            if let Value::String(s) = value {
                if args.is_empty() {
                    return Ok(vec![Value::Bool(false)]);
                }
                let patterns = evaluate(value, &args[0], ctx)?;
                let pattern = match patterns.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Ok(vec![Value::Bool(false)]),
                };
                let flags = if args.len() > 1 {
                    match evaluate(value, &args[1], ctx)?.first() {
                        Some(Value::String(s)) => s.clone(),
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };
                let pat = if flags.contains('i') {
                    format!("(?i){}", pattern)
                } else {
                    pattern
                };
                match regex_lite::Regex::new(&pat) {
                    Ok(re) => Ok(vec![Value::Bool(re.is_match(s))]),
                    Err(_) => Ok(vec![Value::Bool(false)]),
                }
            } else {
                Ok(vec![Value::Bool(false)])
            }
        }

        "match" => {
            if let Value::String(s) = value {
                if args.is_empty() {
                    return Ok(vec![Value::Null]);
                }
                let patterns = evaluate(value, &args[0], ctx)?;
                let pattern = match patterns.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Ok(vec![Value::Null]),
                };
                let flags = if args.len() > 1 {
                    match evaluate(value, &args[1], ctx)?.first() {
                        Some(Value::String(s)) => s.clone(),
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };
                let pat = if flags.contains('i') {
                    format!("(?i){}", pattern)
                } else {
                    pattern
                };
                match regex_lite::Regex::new(&pat) {
                    Ok(re) => {
                        if let Some(m) = re.find(s) {
                            let mut result = IndexMap::new();
                            result.insert(
                                "offset".to_string(),
                                Value::Number(m.start() as f64),
                            );
                            result.insert(
                                "length".to_string(),
                                Value::Number(m.len() as f64),
                            );
                            result.insert(
                                "string".to_string(),
                                Value::String(m.as_str().to_string()),
                            );
                            result.insert(
                                "captures".to_string(),
                                Value::Array(vec![]),
                            );
                            Ok(vec![Value::Object(result)])
                        } else {
                            Ok(vec![])
                        }
                    }
                    Err(_) => Ok(vec![Value::Null]),
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "capture" => {
            if let Value::String(_s) = value {
                // Simplified capture - returns empty object
                Ok(vec![Value::Object(IndexMap::new())])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "sub" => {
            if let Value::String(s) = value {
                if args.len() < 2 {
                    return Ok(vec![Value::Null]);
                }
                let patterns = evaluate(value, &args[0], ctx)?;
                let replacements = evaluate(value, &args[1], ctx)?;
                let pattern = match patterns.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Ok(vec![value.clone()]),
                };
                let replacement = match replacements.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Ok(vec![value.clone()]),
                };
                let flags = if args.len() > 2 {
                    match evaluate(value, &args[2], ctx)?.first() {
                        Some(Value::String(s)) => s.clone(),
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };
                let pat = if flags.contains('i') {
                    format!("(?i){}", pattern)
                } else {
                    pattern
                };
                match regex_lite::Regex::new(&pat) {
                    Ok(re) => Ok(vec![Value::String(
                        re.replace(s, replacement.as_str()).to_string(),
                    )]),
                    Err(_) => Ok(vec![value.clone()]),
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "gsub" => {
            if let Value::String(s) = value {
                if args.len() < 2 {
                    return Ok(vec![Value::Null]);
                }
                let patterns = evaluate(value, &args[0], ctx)?;
                let replacements = evaluate(value, &args[1], ctx)?;
                let pattern = match patterns.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Ok(vec![value.clone()]),
                };
                let replacement = match replacements.first() {
                    Some(Value::String(s)) => s.clone(),
                    _ => return Ok(vec![value.clone()]),
                };
                let flags = if args.len() > 2 {
                    match evaluate(value, &args[2], ctx)?.first() {
                        Some(Value::String(s)) => s.clone(),
                        _ => "g".to_string(),
                    }
                } else {
                    "g".to_string()
                };
                let pat = if flags.contains('i') {
                    format!("(?i){}", pattern)
                } else {
                    pattern
                };
                match regex_lite::Regex::new(&pat) {
                    Ok(re) => Ok(vec![Value::String(
                        re.replace_all(s, replacement.as_str()).to_string(),
                    )]),
                    Err(_) => Ok(vec![value.clone()]),
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "ascii_downcase" => {
            if let Value::String(s) = value {
                Ok(vec![Value::String(s.to_ascii_lowercase())])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "ascii_upcase" => {
            if let Value::String(s) = value {
                Ok(vec![Value::String(s.to_ascii_uppercase())])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "ltrimstr" => {
            if let Value::String(s) = value {
                if args.is_empty() {
                    return Ok(vec![value.clone()]);
                }
                let prefixes = evaluate(value, &args[0], ctx)?;
                let prefix = match prefixes.first() {
                    Some(Value::String(p)) => p.clone(),
                    _ => return Ok(vec![value.clone()]),
                };
                if s.starts_with(&prefix) {
                    Ok(vec![Value::String(s[prefix.len()..].to_string())])
                } else {
                    Ok(vec![value.clone()])
                }
            } else {
                Ok(vec![value.clone()])
            }
        }

        "rtrimstr" => {
            if let Value::String(s) = value {
                if args.is_empty() {
                    return Ok(vec![value.clone()]);
                }
                let suffixes = evaluate(value, &args[0], ctx)?;
                let suffix = match suffixes.first() {
                    Some(Value::String(p)) => p.clone(),
                    _ => return Ok(vec![value.clone()]),
                };
                if suffix.is_empty() {
                    return Ok(vec![value.clone()]);
                }
                if s.ends_with(&suffix) {
                    Ok(vec![Value::String(
                        s[..s.len() - suffix.len()].to_string(),
                    )])
                } else {
                    Ok(vec![value.clone()])
                }
            } else {
                Ok(vec![value.clone()])
            }
        }

        "startswith" => {
            if let Value::String(s) = value {
                if args.is_empty() {
                    return Ok(vec![Value::Bool(false)]);
                }
                let prefixes = evaluate(value, &args[0], ctx)?;
                let prefix = match prefixes.first() {
                    Some(Value::String(p)) => p.clone(),
                    _ => return Ok(vec![Value::Bool(false)]),
                };
                Ok(vec![Value::Bool(s.starts_with(&prefix))])
            } else {
                Ok(vec![Value::Bool(false)])
            }
        }

        "endswith" => {
            if let Value::String(s) = value {
                if args.is_empty() {
                    return Ok(vec![Value::Bool(false)]);
                }
                let suffixes = evaluate(value, &args[0], ctx)?;
                let suffix = match suffixes.first() {
                    Some(Value::String(p)) => p.clone(),
                    _ => return Ok(vec![Value::Bool(false)]),
                };
                Ok(vec![Value::Bool(s.ends_with(&suffix))])
            } else {
                Ok(vec![Value::Bool(false)])
            }
        }

        "trim" => {
            if let Value::String(s) = value {
                Ok(vec![Value::String(s.trim().to_string())])
            } else {
                Err(JqError::Type("trim input must be a string".to_string()))
            }
        }

        "ltrim" => {
            if let Value::String(s) = value {
                Ok(vec![Value::String(s.trim_start().to_string())])
            } else {
                Err(JqError::Type("trim input must be a string".to_string()))
            }
        }

        "rtrim" => {
            if let Value::String(s) = value {
                Ok(vec![Value::String(s.trim_end().to_string())])
            } else {
                Err(JqError::Type("trim input must be a string".to_string()))
            }
        }

        "ascii" => {
            if let Value::String(s) = value {
                if let Some(c) = s.chars().next() {
                    Ok(vec![Value::Number(c as u32 as f64)])
                } else {
                    Ok(vec![Value::Null])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "explode" => {
            if let Value::String(s) = value {
                let codepoints: Vec<Value> = s
                    .chars()
                    .map(|c| Value::Number(c as u32 as f64))
                    .collect();
                Ok(vec![Value::Array(codepoints)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "implode" => {
            if let Value::Array(arr) = value {
                let mut result = String::new();
                for cp in arr {
                    if let Value::Number(n) = cp {
                        let code = *n as u32;
                        if let Some(c) = char::from_u32(code) {
                            result.push(c);
                        } else {
                            result.push('\u{FFFD}');
                        }
                    } else {
                        return Err(JqError::Type(
                            "implode requires numeric codepoints".to_string(),
                        ));
                    }
                }
                Ok(vec![Value::String(result)])
            } else {
                Err(JqError::Type(
                    "implode input must be an array".to_string(),
                ))
            }
        }

        // ===== Path builtins =====
        "getpath" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            let paths = evaluate(value, &args[0], ctx)?;
            let mut results = Vec::new();
            for path_val in &paths {
                if let Value::Array(path) = path_val {
                    let mut current = value.clone();
                    for key in path {
                        match (&current, key) {
                            (Value::Array(arr), Value::Number(n)) => {
                                let i = *n as usize;
                                current = arr.get(i).cloned().unwrap_or(Value::Null);
                            }
                            (Value::Object(obj), Value::String(s)) => {
                                current = obj.get(s).cloned().unwrap_or(Value::Null);
                            }
                            _ => {
                                current = Value::Null;
                                break;
                            }
                        }
                    }
                    results.push(current);
                } else {
                    results.push(Value::Null);
                }
            }
            Ok(results)
        }

        "setpath" => {
            if args.len() < 2 {
                return Ok(vec![Value::Null]);
            }
            let paths = evaluate(value, &args[0], ctx)?;
            let path = paths.first().cloned().unwrap_or(Value::Null);
            let vals = evaluate(value, &args[1], ctx)?;
            let new_val = vals.first().cloned().unwrap_or(Value::Null);
            if let Value::Array(path_arr) = &path {
                let path_elems: Vec<PathElement> = path_arr
                    .iter()
                    .map(|p| match p {
                        Value::String(s) => PathElement::Key(s.clone()),
                        Value::Number(n) => PathElement::Index(*n as i64),
                        _ => PathElement::Key(format!("{}", p)),
                    })
                    .collect();
                Ok(vec![set_path(value, &path_elems, new_val)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "delpaths" => {
            if args.is_empty() {
                return Ok(vec![value.clone()]);
            }
            let path_lists = evaluate(value, &args[0], ctx)?;
            let paths = path_lists.first().cloned().unwrap_or(Value::Null);
            if let Value::Array(path_arr) = &paths {
                let mut sorted_paths: Vec<Vec<PathElement>> = path_arr
                    .iter()
                    .filter_map(|p| {
                        if let Value::Array(path) = p {
                            Some(
                                path.iter()
                                    .map(|elem| match elem {
                                        Value::String(s) => PathElement::Key(s.clone()),
                                        Value::Number(n) => PathElement::Index(*n as i64),
                                        _ => PathElement::Key(format!("{}", elem)),
                                    })
                                    .collect(),
                            )
                        } else {
                            None
                        }
                    })
                    .collect();
                sorted_paths.sort_by(|a, b| b.len().cmp(&a.len()));
                let mut result = value.clone();
                for path in &sorted_paths {
                    result = delete_path(&result, path);
                }
                Ok(vec![result])
            } else {
                Ok(vec![value.clone()])
            }
        }

        "path" => {
            if args.is_empty() {
                return Ok(vec![Value::Array(vec![])]);
            }
            let mut paths = Vec::new();
            collect_paths(value, &args[0], ctx, &[], &mut paths)?;
            Ok(paths
                .into_iter()
                .map(|p| {
                    Value::Array(
                        p.into_iter()
                            .map(|elem| match elem {
                                PathElement::Key(k) => Value::String(k),
                                PathElement::Index(i) => Value::Number(i as f64),
                            })
                            .collect(),
                    )
                })
                .collect())
        }

        "del" => {
            if args.is_empty() {
                return Ok(vec![value.clone()]);
            }
            Ok(vec![apply_del(value, &args[0], ctx)?])
        }

        "pick" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            let mut all_paths = Vec::new();
            for arg in args {
                collect_paths(value, arg, ctx, &[], &mut all_paths)?;
            }
            let mut result = Value::Null;
            for path in &all_paths {
                let mut current = value.clone();
                for elem in path {
                    match (&current, elem) {
                        (Value::Array(arr), PathElement::Index(i)) => {
                            let idx = *i as usize;
                            current = arr.get(idx).cloned().unwrap_or(Value::Null);
                        }
                        (Value::Object(obj), PathElement::Key(k)) => {
                            current = obj.get(k).cloned().unwrap_or(Value::Null);
                        }
                        _ => {
                            current = Value::Null;
                            break;
                        }
                    }
                }
                result = set_path(&result, path, current);
            }
            Ok(vec![result])
        }

        "paths" => {
            let mut paths = Vec::new();
            collect_all_paths(value, &[], &mut paths);
            if !args.is_empty() {
                let mut filtered = Vec::new();
                for path in &paths {
                    let mut v = value.clone();
                    for elem in path {
                        match (&v, elem) {
                            (Value::Array(arr), PathElement::Index(i)) => {
                                v = arr.get(*i as usize).cloned().unwrap_or(Value::Null);
                            }
                            (Value::Object(obj), PathElement::Key(k)) => {
                                v = obj.get(k).cloned().unwrap_or(Value::Null);
                            }
                            _ => {
                                v = Value::Null;
                                break;
                            }
                        }
                    }
                    let results = evaluate(&v, &args[0], ctx)?;
                    if results.iter().any(|r| r.is_truthy()) {
                        filtered.push(path.clone());
                    }
                }
                paths = filtered;
            }
            Ok(paths
                .into_iter()
                .map(|p| {
                    Value::Array(
                        p.into_iter()
                            .map(|elem| match elem {
                                PathElement::Key(k) => Value::String(k),
                                PathElement::Index(i) => Value::Number(i as f64),
                            })
                            .collect(),
                    )
                })
                .collect())
        }

        "leaf_paths" => {
            let mut paths = Vec::new();
            collect_leaf_paths(value, &[], &mut paths);
            Ok(paths
                .into_iter()
                .map(|p| {
                    Value::Array(
                        p.into_iter()
                            .map(|elem| match elem {
                                PathElement::Key(k) => Value::String(k),
                                PathElement::Index(i) => Value::Number(i as f64),
                            })
                            .collect(),
                    )
                })
                .collect())
        }

        // ===== Index builtins =====
        "index" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            let needles = evaluate(value, &args[0], ctx)?;
            let mut results = Vec::new();
            for needle in &needles {
                match (value, needle) {
                    (Value::String(s), Value::String(n)) => {
                        if n.is_empty() && s.is_empty() {
                            results.push(Value::Null);
                        } else if let Some(idx) = s.find(n.as_str()) {
                            results.push(Value::Number(idx as f64));
                        } else {
                            results.push(Value::Null);
                        }
                    }
                    (Value::Array(arr), _) => {
                        if let Value::Array(needle_arr) = needle {
                            let mut found = false;
                            let nlen = needle_arr.len();
                            if nlen <= arr.len() {
                                for i in 0..=(arr.len() - nlen) {
                                    let mut matched = true;
                                    for j in 0..nlen {
                                        if !deep_equal(&arr[i + j], &needle_arr[j]) {
                                            matched = false;
                                            break;
                                        }
                                    }
                                    if matched {
                                        results.push(Value::Number(i as f64));
                                        found = true;
                                        break;
                                    }
                                }
                            }
                            if !found {
                                results.push(Value::Null);
                            }
                        } else {
                            let idx = arr.iter().position(|x| deep_equal(x, needle));
                            results.push(match idx {
                                Some(i) => Value::Number(i as f64),
                                None => Value::Null,
                            });
                        }
                    }
                    _ => results.push(Value::Null),
                }
            }
            Ok(results)
        }

        "rindex" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            let needles = evaluate(value, &args[0], ctx)?;
            let mut results = Vec::new();
            for needle in &needles {
                match (value, needle) {
                    (Value::String(s), Value::String(n)) => {
                        if let Some(idx) = s.rfind(n.as_str()) {
                            results.push(Value::Number(idx as f64));
                        } else {
                            results.push(Value::Null);
                        }
                    }
                    (Value::Array(arr), _) => {
                        if let Value::Array(needle_arr) = needle {
                            let mut found = false;
                            let nlen = needle_arr.len();
                            if nlen <= arr.len() {
                                for i in (0..=(arr.len() - nlen)).rev() {
                                    let mut matched = true;
                                    for j in 0..nlen {
                                        if !deep_equal(&arr[i + j], &needle_arr[j]) {
                                            matched = false;
                                            break;
                                        }
                                    }
                                    if matched {
                                        results.push(Value::Number(i as f64));
                                        found = true;
                                        break;
                                    }
                                }
                            }
                            if !found {
                                results.push(Value::Null);
                            }
                        } else {
                            let mut found = false;
                            for i in (0..arr.len()).rev() {
                                if deep_equal(&arr[i], needle) {
                                    results.push(Value::Number(i as f64));
                                    found = true;
                                    break;
                                }
                            }
                            if !found {
                                results.push(Value::Null);
                            }
                        }
                    }
                    _ => results.push(Value::Null),
                }
            }
            Ok(results)
        }

        "indices" => {
            if args.is_empty() {
                return Ok(vec![Value::Array(vec![])]);
            }
            let needles = evaluate(value, &args[0], ctx)?;
            let mut results = Vec::new();
            for needle in &needles {
                let mut indices = Vec::new();
                match (value, needle) {
                    (Value::String(s), Value::String(n)) => {
                        let mut start = 0;
                        while let Some(idx) = s[start..].find(n.as_str()) {
                            indices.push(Value::Number((start + idx) as f64));
                            start += idx + 1;
                        }
                    }
                    (Value::Array(arr), _) => {
                        if let Value::Array(needle_arr) = needle {
                            let nlen = needle_arr.len();
                            if nlen == 0 {
                                for i in 0..=arr.len() {
                                    indices.push(Value::Number(i as f64));
                                }
                            } else if nlen <= arr.len() {
                                for i in 0..=(arr.len() - nlen) {
                                    let mut matched = true;
                                    for j in 0..nlen {
                                        if !deep_equal(&arr[i + j], &needle_arr[j]) {
                                            matched = false;
                                            break;
                                        }
                                    }
                                    if matched {
                                        indices.push(Value::Number(i as f64));
                                    }
                                }
                            }
                        } else {
                            for (i, item) in arr.iter().enumerate() {
                                if deep_equal(item, needle) {
                                    indices.push(Value::Number(i as f64));
                                }
                            }
                        }
                    }
                    _ => {}
                }
                results.push(Value::Array(indices));
            }
            Ok(results)
        }

        // ===== Control flow builtins =====
        "first" => {
            if !args.is_empty() {
                let results = evaluate(value, &args[0], ctx).unwrap_or_default();
                if results.is_empty() {
                    Ok(vec![])
                } else {
                    Ok(vec![results[0].clone()])
                }
            } else if let Value::Array(arr) = value {
                if arr.is_empty() {
                    Ok(vec![Value::Null])
                } else {
                    Ok(vec![arr[0].clone()])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "last" => {
            if !args.is_empty() {
                let results = evaluate(value, &args[0], ctx)?;
                if results.is_empty() {
                    Ok(vec![])
                } else {
                    Ok(vec![results.last().unwrap().clone()])
                }
            } else if let Value::Array(arr) = value {
                if arr.is_empty() {
                    Ok(vec![Value::Null])
                } else {
                    Ok(vec![arr.last().unwrap().clone()])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "nth" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            let ns = evaluate(value, &args[0], ctx)?;
            if args.len() > 1 {
                let results = evaluate(value, &args[1], ctx).unwrap_or_default();
                let mut output = Vec::new();
                for nv in &ns {
                    if let Value::Number(n) = nv {
                        let i = *n as i64;
                        if i < 0 {
                            return Err(JqError::Runtime(
                                "nth doesn't support negative indices".to_string(),
                            ));
                        }
                        if (i as usize) < results.len() {
                            output.push(results[i as usize].clone());
                        }
                    }
                }
                Ok(output)
            } else if let Value::Array(arr) = value {
                let mut output = Vec::new();
                for nv in &ns {
                    if let Value::Number(n) = nv {
                        let i = *n as i64;
                        if i < 0 {
                            return Err(JqError::Runtime(
                                "nth doesn't support negative indices".to_string(),
                            ));
                        }
                        if (i as usize) < arr.len() {
                            output.push(arr[i as usize].clone());
                        } else {
                            output.push(Value::Null);
                        }
                    }
                }
                Ok(output)
            } else {
                Ok(vec![Value::Null])
            }
        }

        "range" => {
            if args.is_empty() {
                return Ok(vec![]);
            }
            let starts_vals = evaluate(value, &args[0], ctx)?;
            if args.len() == 1 {
                let mut result = Vec::new();
                for n in &starts_vals {
                    if let Value::Number(num) = n {
                        let end = *num as i64;
                        for i in 0..end {
                            result.push(Value::Number(i as f64));
                        }
                    }
                }
                return Ok(result);
            }
            let ends_vals = evaluate(value, &args[1], ctx)?;
            if args.len() == 2 {
                let mut result = Vec::new();
                for s in &starts_vals {
                    for e in &ends_vals {
                        if let (Value::Number(start), Value::Number(end)) = (s, e) {
                            let mut i = *start;
                            while i < *end {
                                result.push(Value::Number(i));
                                i += 1.0;
                            }
                        }
                    }
                }
                return Ok(result);
            }
            let steps_vals = evaluate(value, &args[2], ctx)?;
            let mut result = Vec::new();
            for s in &starts_vals {
                for e in &ends_vals {
                    for st in &steps_vals {
                        if let (Value::Number(start), Value::Number(end), Value::Number(step)) =
                            (s, e, st)
                        {
                            if *step == 0.0 {
                                continue;
                            }
                            let mut i = *start;
                            if *step > 0.0 {
                                while i < *end {
                                    result.push(Value::Number(i));
                                    i += step;
                                }
                            } else {
                                while i > *end {
                                    result.push(Value::Number(i));
                                    i += step;
                                }
                            }
                        }
                    }
                }
            }
            Ok(result)
        }

        "limit" => {
            if args.len() < 2 {
                return Ok(vec![]);
            }
            let ns = evaluate(value, &args[0], ctx)?;
            let mut output = Vec::new();
            for nv in &ns {
                if let Value::Number(n) = nv {
                    let limit = *n as usize;
                    if limit == 0 {
                        continue;
                    }
                    let results = evaluate(value, &args[1], ctx).unwrap_or_default();
                    output.extend(results.into_iter().take(limit));
                }
            }
            Ok(output)
        }

        "isempty" => {
            if args.is_empty() {
                return Ok(vec![Value::Bool(true)]);
            }
            match evaluate(value, &args[0], ctx) {
                Ok(results) => Ok(vec![Value::Bool(results.is_empty())]),
                Err(_) => Ok(vec![Value::Bool(true)]),
            }
        }

        "isvalid" => {
            if args.is_empty() {
                return Ok(vec![Value::Bool(true)]);
            }
            match evaluate(value, &args[0], ctx) {
                Ok(results) => Ok(vec![Value::Bool(!results.is_empty())]),
                Err(_) => Ok(vec![Value::Bool(false)]),
            }
        }

        "skip" => {
            if args.len() < 2 {
                return Ok(vec![]);
            }
            let ns = evaluate(value, &args[0], ctx)?;
            let mut output = Vec::new();
            for nv in &ns {
                if let Value::Number(n) = nv {
                    let skip = *n as usize;
                    let results = evaluate(value, &args[1], ctx)?;
                    output.extend(results.into_iter().skip(skip));
                }
            }
            Ok(output)
        }

        "until" => {
            if args.len() < 2 {
                return Ok(vec![value.clone()]);
            }
            let mut current = value.clone();
            let max_iter = ctx.max_iterations;
            for _ in 0..max_iter {
                let conds = evaluate(&current, &args[0], ctx)?;
                if conds.iter().any(|c| c.is_truthy()) {
                    return Ok(vec![current]);
                }
                let next = evaluate(&current, &args[1], ctx)?;
                current = next.into_iter().next().unwrap_or(current);
            }
            Err(JqError::ExecutionLimit(format!(
                "until: too many iterations ({})",
                max_iter
            )))
        }

        "while" => {
            if args.len() < 2 {
                return Ok(vec![value.clone()]);
            }
            let mut results = Vec::new();
            let mut current = value.clone();
            let max_iter = ctx.max_iterations;
            for _ in 0..max_iter {
                let conds = evaluate(&current, &args[0], ctx)?;
                if !conds.iter().any(|c| c.is_truthy()) {
                    break;
                }
                results.push(current.clone());
                let next = evaluate(&current, &args[1], ctx)?;
                if next.is_empty() {
                    break;
                }
                current = next.into_iter().next().unwrap();
            }
            Ok(results)
        }

        "repeat" => {
            if args.is_empty() {
                return Ok(vec![value.clone()]);
            }
            let mut results = Vec::new();
            let mut current = value.clone();
            let max_iter = ctx.max_iterations;
            for _ in 0..max_iter {
                results.push(current.clone());
                let next = evaluate(&current, &args[0], ctx)?;
                if next.is_empty() {
                    break;
                }
                current = next.into_iter().next().unwrap();
            }
            Ok(results)
        }

        // ===== Navigation builtins =====
        "recurse" => {
            if args.is_empty() {
                let mut results = Vec::new();
                recurse_walk(value, &mut results);
                return Ok(results);
            }
            let cond_expr = if args.len() >= 2 { Some(&args[1]) } else { None };
            let mut results = Vec::new();
            let max_depth = 10000;
            recurse_with_filter(value, &args[0], cond_expr, ctx, &mut results, 0, max_depth)?;
            Ok(results)
        }

        "recurse_down" => eval_builtin(value, "recurse", args, ctx),

        "walk" => {
            if args.is_empty() {
                return Ok(vec![value.clone()]);
            }
            let walked = walk_value(value, &args[0], ctx)?;
            Ok(vec![walked])
        }

        "transpose" => {
            if let Value::Array(arr) = value {
                if arr.is_empty() {
                    return Ok(vec![Value::Array(vec![])]);
                }
                let max_len = arr
                    .iter()
                    .map(|row| {
                        if let Value::Array(r) = row {
                            r.len()
                        } else {
                            0
                        }
                    })
                    .max()
                    .unwrap_or(0);
                let mut result = Vec::new();
                for i in 0..max_len {
                    let col: Vec<Value> = arr
                        .iter()
                        .map(|row| {
                            if let Value::Array(r) = row {
                                r.get(i).cloned().unwrap_or(Value::Null)
                            } else {
                                Value::Null
                            }
                        })
                        .collect();
                    result.push(Value::Array(col));
                }
                Ok(vec![Value::Array(result)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "combinations" => {
            if !args.is_empty() {
                let ns = evaluate(value, &args[0], ctx)?;
                let n = match ns.first() {
                    Some(Value::Number(n)) => *n as usize,
                    _ => return Ok(vec![]),
                };
                if let Value::Array(arr) = value {
                    if n == 0 {
                        return Ok(vec![Value::Array(vec![])]);
                    }
                    let mut results = Vec::new();
                    generate_combinations(arr, n, &mut vec![], &mut results);
                    return Ok(results);
                }
                return Ok(vec![]);
            }
            if let Value::Array(arr) = value {
                if arr.is_empty() {
                    return Ok(vec![Value::Array(vec![])]);
                }
                for item in arr.iter() {
                    if !matches!(item, Value::Array(_)) {
                        return Ok(vec![]);
                    }
                }
                let mut results = Vec::new();
                generate_cartesian(arr, 0, &mut vec![], &mut results);
                Ok(results)
            } else {
                Ok(vec![])
            }
        }

        "parent" => {
            // Simplified - returns empty without path tracking
            Ok(vec![])
        }

        "parents" => Ok(vec![Value::Array(vec![])]),

        "root" => {
            if let Some(root) = &ctx.root {
                Ok(vec![root.clone()])
            } else {
                Ok(vec![])
            }
        }

        // ===== Math builtins =====
        "fabs" | "abs" => match value {
            Value::Number(n) => Ok(vec![Value::Number(n.abs())]),
            Value::String(_) => Ok(vec![value.clone()]),
            _ => Ok(vec![Value::Null]),
        },

        "pow" => {
            if args.len() < 2 {
                return Ok(vec![Value::Null]);
            }
            let bases = evaluate(value, &args[0], ctx)?;
            let exps = evaluate(value, &args[1], ctx)?;
            match (bases.first(), exps.first()) {
                (Some(Value::Number(b)), Some(Value::Number(e))) => {
                    Ok(vec![Value::Number(b.powf(*e))])
                }
                _ => Ok(vec![Value::Null]),
            }
        }

        "atan2" => {
            if args.len() < 2 {
                return Ok(vec![Value::Null]);
            }
            let ys = evaluate(value, &args[0], ctx)?;
            let xs = evaluate(value, &args[1], ctx)?;
            match (ys.first(), xs.first()) {
                (Some(Value::Number(y)), Some(Value::Number(x))) => {
                    Ok(vec![Value::Number(y.atan2(*x))])
                }
                _ => Ok(vec![Value::Null]),
            }
        }

        "hypot" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            if let Value::Number(x) = value {
                let ys = evaluate(value, &args[0], ctx)?;
                if let Some(Value::Number(y)) = ys.first() {
                    Ok(vec![Value::Number(x.hypot(*y))])
                } else {
                    Ok(vec![Value::Null])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "fma" => {
            if args.len() < 2 {
                return Ok(vec![Value::Null]);
            }
            if let Value::Number(x) = value {
                let ys = evaluate(value, &args[0], ctx)?;
                let zs = evaluate(value, &args[1], ctx)?;
                match (ys.first(), zs.first()) {
                    (Some(Value::Number(y)), Some(Value::Number(z))) => {
                        Ok(vec![Value::Number(x * y + z)])
                    }
                    _ => Ok(vec![Value::Null]),
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "copysign" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            if let Value::Number(x) = value {
                let ys = evaluate(value, &args[0], ctx)?;
                if let Some(Value::Number(y)) = ys.first() {
                    Ok(vec![Value::Number(y.signum() * x.abs())])
                } else {
                    Ok(vec![Value::Null])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "drem" | "remainder" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            if let Value::Number(x) = value {
                let ys = evaluate(value, &args[0], ctx)?;
                if let Some(Value::Number(y)) = ys.first() {
                    Ok(vec![Value::Number(x - (x / y).round() * y)])
                } else {
                    Ok(vec![Value::Null])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "fdim" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            if let Value::Number(x) = value {
                let ys = evaluate(value, &args[0], ctx)?;
                if let Some(Value::Number(y)) = ys.first() {
                    Ok(vec![Value::Number((x - y).max(0.0))])
                } else {
                    Ok(vec![Value::Null])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "fmax" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            if let Value::Number(x) = value {
                let ys = evaluate(value, &args[0], ctx)?;
                if let Some(Value::Number(y)) = ys.first() {
                    Ok(vec![Value::Number(x.max(*y))])
                } else {
                    Ok(vec![Value::Null])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "fmin" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            if let Value::Number(x) = value {
                let ys = evaluate(value, &args[0], ctx)?;
                if let Some(Value::Number(y)) = ys.first() {
                    Ok(vec![Value::Number(x.min(*y))])
                } else {
                    Ok(vec![Value::Null])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "ldexp" | "scalbn" | "scalbln" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            if let Value::Number(x) = value {
                let exps = evaluate(value, &args[0], ctx)?;
                if let Some(Value::Number(e)) = exps.first() {
                    Ok(vec![Value::Number(x * 2.0_f64.powf(*e))])
                } else {
                    Ok(vec![Value::Null])
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "nearbyint" => {
            if let Value::Number(n) = value {
                Ok(vec![Value::Number(n.round())])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "logb" => {
            if let Value::Number(n) = value {
                Ok(vec![Value::Number(n.abs().log2().floor())])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "significand" => {
            if let Value::Number(n) = value {
                let exp = n.abs().log2().floor();
                Ok(vec![Value::Number(n / 2.0_f64.powf(exp))])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "frexp" => {
            if let Value::Number(n) = value {
                if *n == 0.0 {
                    return Ok(vec![Value::Array(vec![
                        Value::Number(0.0),
                        Value::Number(0.0),
                    ])]);
                }
                let exp = n.abs().log2().floor() as i64 + 1;
                let mantissa = n / 2.0_f64.powi(exp as i32);
                Ok(vec![Value::Array(vec![
                    Value::Number(mantissa),
                    Value::Number(exp as f64),
                ])])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "modf" => {
            if let Value::Number(n) = value {
                let int_part = n.trunc();
                let frac_part = n - int_part;
                Ok(vec![Value::Array(vec![
                    Value::Number(frac_part),
                    Value::Number(int_part),
                ])])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "exp10" => {
            if let Value::Number(n) = value {
                Ok(vec![Value::Number(10.0_f64.powf(*n))])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "exp2" => {
            if let Value::Number(n) = value {
                Ok(vec![Value::Number(2.0_f64.powf(*n))])
            } else {
                Ok(vec![Value::Null])
            }
        }

        // ===== Format builtins =====
        "@base64" => {
            if let Value::String(s) = value {
                use base64::Engine;
                Ok(vec![Value::String(
                    base64::engine::general_purpose::STANDARD.encode(s.as_bytes()),
                )])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "@base64d" => {
            if let Value::String(s) = value {
                use base64::Engine;
                match base64::engine::general_purpose::STANDARD.decode(s.as_bytes()) {
                    Ok(bytes) => Ok(vec![Value::String(
                        String::from_utf8_lossy(&bytes).to_string(),
                    )]),
                    Err(_) => Err(JqError::Runtime("invalid base64 input".to_string())),
                }
            } else {
                Ok(vec![Value::Null])
            }
        }

        "@uri" => {
            if let Value::String(s) = value {
                let encoded = percent_encode(s);
                Ok(vec![Value::String(encoded)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "@csv" => {
            if let Value::Array(arr) = value {
                let csv_parts: Vec<String> = arr
                    .iter()
                    .map(|v| match v {
                        Value::Null => String::new(),
                        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
                        Value::Number(n) => format_number(*n),
                        Value::String(s) => {
                            if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
                                format!("\"{}\"", s.replace('"', "\"\""))
                            } else {
                                s.clone()
                            }
                        }
                        _ => v.to_json_string_compact(),
                    })
                    .collect();
                Ok(vec![Value::String(csv_parts.join(","))])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "@tsv" => {
            if let Value::Array(arr) = value {
                let tsv_parts: Vec<String> = arr
                    .iter()
                    .map(|v| {
                        let s = match v {
                            Value::Null => String::new(),
                            Value::String(s) => s.clone(),
                            _ => format!("{}", v),
                        };
                        s.replace('\t', "\\t").replace('\n', "\\n")
                    })
                    .collect();
                Ok(vec![Value::String(tsv_parts.join("\t"))])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "@json" => Ok(vec![Value::String(value.to_json_string_compact())]),

        "@html" => {
            if let Value::String(s) = value {
                let escaped = s
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('\'', "&apos;")
                    .replace('"', "&quot;");
                Ok(vec![Value::String(escaped)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "@sh" => {
            if let Value::String(s) = value {
                Ok(vec![Value::String(format!(
                    "'{}'",
                    s.replace('\'', "'\\''")
                ))])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "@text" => match value {
            Value::String(_) => Ok(vec![value.clone()]),
            Value::Null => Ok(vec![Value::String(String::new())]),
            _ => Ok(vec![Value::String(format!("{}", value))]),
        },

        // ===== Date builtins =====
        "now" => {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            Ok(vec![Value::Number(secs)])
        }

        "gmtime" => {
            if let Value::Number(ts) = value {
                let secs = *ts as i64;
                let dt = chrono::DateTime::from_timestamp(secs, 0)
                    .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
                use chrono::Datelike;
                use chrono::Timelike;
                let year = dt.year() as f64;
                let month = (dt.month0()) as f64;
                let day = dt.day() as f64;
                let hour = dt.hour() as f64;
                let minute = dt.minute() as f64;
                let second = dt.second() as f64;
                let weekday = dt.weekday().num_days_from_sunday() as f64;
                let yearday = dt.ordinal0() as f64;
                Ok(vec![Value::Array(vec![
                    Value::Number(year),
                    Value::Number(month),
                    Value::Number(day),
                    Value::Number(hour),
                    Value::Number(minute),
                    Value::Number(second),
                    Value::Number(weekday),
                    Value::Number(yearday),
                ])])
            } else {
                Ok(vec![Value::Null])
            }
        }

        "mktime" => {
            if let Value::Array(arr) = value {
                if arr.len() < 2 {
                    return Err(JqError::Runtime(
                        "mktime requires parsed datetime inputs".to_string(),
                    ));
                }
                let year = match &arr[0] {
                    Value::Number(n) => *n as i32,
                    _ => return Err(JqError::Runtime("mktime requires numeric inputs".to_string())),
                };
                let month = match &arr[1] {
                    Value::Number(n) => *n as u32,
                    _ => return Err(JqError::Runtime("mktime requires numeric inputs".to_string())),
                };
                let day = arr.get(2).and_then(|v| if let Value::Number(n) = v { Some(*n as u32) } else { None }).unwrap_or(1);
                let hour = arr.get(3).and_then(|v| if let Value::Number(n) = v { Some(*n as u32) } else { None }).unwrap_or(0);
                let minute = arr.get(4).and_then(|v| if let Value::Number(n) = v { Some(*n as u32) } else { None }).unwrap_or(0);
                let second = arr.get(5).and_then(|v| if let Value::Number(n) = v { Some(*n as u32) } else { None }).unwrap_or(0);
                use chrono::TimeZone;
                if let Some(dt) = chrono::Utc.with_ymd_and_hms(year, month + 1, day, hour, minute, second).single() {
                    Ok(vec![Value::Number(dt.timestamp() as f64)])
                } else {
                    Err(JqError::Runtime("invalid datetime".to_string()))
                }
            } else {
                Err(JqError::Runtime("mktime requires parsed datetime inputs".to_string()))
            }
        }

        "strftime" => {
            if args.is_empty() {
                return Ok(vec![Value::Null]);
            }
            let fmt_vals = evaluate(value, &args[0], ctx)?;
            let fmt = match fmt_vals.first() {
                Some(Value::String(s)) => s.clone(),
                _ => return Err(JqError::Runtime("strftime requires a string format".to_string())),
            };
            let ts = match value {
                Value::Number(n) => *n as i64,
                Value::Array(arr) => {
                    // Broken-down time array
                    let year = match arr.get(0) { Some(Value::Number(n)) => *n as i32, _ => 1970 };
                    let month = match arr.get(1) { Some(Value::Number(n)) => *n as u32, _ => 0 };
                    let day = match arr.get(2) { Some(Value::Number(n)) => *n as u32, _ => 1 };
                    let hour = match arr.get(3) { Some(Value::Number(n)) => *n as u32, _ => 0 };
                    let minute = match arr.get(4) { Some(Value::Number(n)) => *n as u32, _ => 0 };
                    let second = match arr.get(5) { Some(Value::Number(n)) => *n as u32, _ => 0 };
                    use chrono::TimeZone;
                    if let Some(dt) = chrono::Utc.with_ymd_and_hms(year, month + 1, day, hour, minute, second).single() {
                        dt.timestamp()
                    } else {
                        0
                    }
                }
                _ => return Err(JqError::Runtime("strftime requires parsed datetime inputs".to_string())),
            };
            let dt = chrono::DateTime::from_timestamp(ts, 0)
                .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
            Ok(vec![Value::String(dt.format(&fmt).to_string())])
        }

        "todate" => {
            if let Value::Number(ts) = value {
                let secs = *ts as i64;
                let dt = chrono::DateTime::from_timestamp(secs, 0)
                    .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
                Ok(vec![Value::String(
                    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                )])
            } else {
                Err(JqError::Runtime("todate requires a number input".to_string()))
            }
        }

        "fromdate" => {
            if let Value::String(s) = value {
                match chrono::DateTime::parse_from_rfc3339(s) {
                    Ok(dt) => Ok(vec![Value::Number(dt.timestamp() as f64)]),
                    Err(_) => {
                        // Try ISO 8601 without timezone
                        match chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
                            Ok(dt) => Ok(vec![Value::Number(dt.and_utc().timestamp() as f64)]),
                            Err(_) => Err(JqError::Runtime(format!(
                                "date \"{}\" does not match format",
                                s
                            ))),
                        }
                    }
                }
            } else {
                Err(JqError::Runtime("fromdate requires a string input".to_string()))
            }
        }

        // ===== SQL-like builtins =====
        "IN" => {
            if args.is_empty() {
                return Ok(vec![Value::Bool(false)]);
            }
            if args.len() == 1 {
                let stream_vals = evaluate(value, &args[0], ctx)?;
                for v in &stream_vals {
                    if deep_equal(value, v) {
                        return Ok(vec![Value::Bool(true)]);
                    }
                }
                return Ok(vec![Value::Bool(false)]);
            }
            let stream1 = evaluate(value, &args[0], ctx)?;
            let stream2 = evaluate(value, &args[1], ctx)?;
            let stream2_set: HashSet<String> = stream2
                .iter()
                .map(|v| v.to_json_string_compact())
                .collect();
            for v in &stream1 {
                if stream2_set.contains(&v.to_json_string_compact()) {
                    return Ok(vec![Value::Bool(true)]);
                }
            }
            Ok(vec![Value::Bool(false)])
        }

        "INDEX" => {
            if args.is_empty() {
                return Ok(vec![Value::Object(IndexMap::new())]);
            }
            if args.len() == 1 {
                let stream_vals = evaluate(value, &args[0], ctx)?;
                let mut result = IndexMap::new();
                for v in &stream_vals {
                    let key = match v {
                        Value::String(s) => s.clone(),
                        _ => format!("{}", v),
                    };
                    result.insert(key, v.clone());
                }
                return Ok(vec![Value::Object(result)]);
            }
            if args.len() == 2 {
                let stream_vals = evaluate(value, &args[0], ctx)?;
                let mut result = IndexMap::new();
                for v in &stream_vals {
                    let keys = evaluate(v, &args[1], ctx)?;
                    if let Some(k) = keys.first() {
                        let key = match k {
                            Value::String(s) => s.clone(),
                            _ => format!("{}", k),
                        };
                        result.insert(key, v.clone());
                    }
                }
                return Ok(vec![Value::Object(result)]);
            }
            let stream_vals = evaluate(value, &args[0], ctx)?;
            let mut result = IndexMap::new();
            for v in &stream_vals {
                let keys = evaluate(v, &args[1], ctx)?;
                let vals = evaluate(v, &args[2], ctx)?;
                if let (Some(k), Some(val)) = (keys.first(), vals.first()) {
                    let key = match k {
                        Value::String(s) => s.clone(),
                        _ => format!("{}", k),
                    };
                    result.insert(key, val.clone());
                }
            }
            Ok(vec![Value::Object(result)])
        }

        "JOIN" => {
            if args.len() < 2 {
                return Ok(vec![Value::Null]);
            }
            let idx = evaluate(value, &args[0], ctx)?
                .into_iter()
                .next()
                .unwrap_or(Value::Null);
            if let (Value::Array(arr), Value::Object(idx_obj)) = (value, &idx) {
                let mut results = Vec::new();
                for item in arr {
                    let keys = evaluate(item, &args[1], ctx)?;
                    let key = keys
                        .first()
                        .map(|k| match k {
                            Value::String(s) => s.clone(),
                            _ => format!("{}", k),
                        })
                        .unwrap_or_default();
                    let lookup = idx_obj.get(&key).cloned().unwrap_or(Value::Null);
                    results.push(Value::Array(vec![item.clone(), lookup]));
                }
                Ok(vec![Value::Array(results)])
            } else {
                Ok(vec![Value::Null])
            }
        }

        // ===== Error/debug builtins =====
        "error" => {
            if !args.is_empty() {
                let msgs = evaluate(value, &args[0], ctx)?;
                let msg = msgs.first().cloned().unwrap_or(Value::Null);
                Err(JqError::Value(msg))
            } else {
                match value {
                    Value::String(s) => Err(JqError::Value(Value::String(s.clone()))),
                    _ => Err(JqError::Value(value.clone())),
                }
            }
        }

        "debug" => {
            // In production, debug just passes through the value
            // Optionally with a message prefix
            if !args.is_empty() {
                let _msgs = evaluate(value, &args[0], ctx).ok();
            }
            Ok(vec![value.clone()])
        }

        "env" | "$ENV" => {
            let mut map = IndexMap::new();
            for (k, v) in &ctx.env {
                map.insert(k.clone(), Value::String(v.clone()));
            }
            Ok(vec![Value::Object(map)])
        }

        "builtins" => {
            let builtin_names = get_builtin_names();
            Ok(vec![Value::Array(
                builtin_names
                    .into_iter()
                    .map(|s| Value::String(s.to_string()))
                    .collect(),
            )])
        }

        "input_line_number" => Ok(vec![Value::Number(0.0)]),

        // ===== Stream builtins =====
        "tostream" => {
            let mut results = Vec::new();
            collect_stream_pairs(value, &[], &mut results);
            results.push(Value::Array(vec![Value::Array(vec![])]));
            Ok(results)
        }

        "fromstream" => {
            if args.is_empty() {
                return Ok(vec![value.clone()]);
            }
            let stream_items = evaluate(value, &args[0], ctx)?;
            let mut result = Value::Null;
            for item in &stream_items {
                if let Value::Array(pair) = item {
                    if pair.len() == 1 {
                        if let Value::Array(inner) = &pair[0] {
                            if inner.is_empty() {
                                continue;
                            }
                        }
                    }
                    if pair.len() == 2 {
                        if let Value::Array(path) = &pair[0] {
                            let path_elems: Vec<PathElement> = path
                                .iter()
                                .map(|p| match p {
                                    Value::String(s) => PathElement::Key(s.clone()),
                                    Value::Number(n) => PathElement::Index(*n as i64),
                                    _ => PathElement::Key(format!("{}", p)),
                                })
                                .collect();
                            if path_elems.is_empty() {
                                result = pair[1].clone();
                            } else {
                                if matches!(result, Value::Null) {
                                    result = match &path_elems[0] {
                                        PathElement::Index(_) => Value::Array(vec![]),
                                        PathElement::Key(_) => Value::Object(IndexMap::new()),
                                    };
                                }
                                result = set_path(&result, &path_elems, pair[1].clone());
                            }
                        }
                    }
                }
            }
            Ok(vec![result])
        }

        // ===== User-defined function dispatch =====
        _ => {
            let func_key = format!("{}/{}", name, args.len());
            if let Some(func_def) = ctx.funcs.get(&func_key).cloned() {
                let mut new_ctx = ctx.clone();
                if let Some(closure) = &func_def.closure {
                    for (k, v) in closure {
                        new_ctx.funcs.insert(k.clone(), v.clone());
                    }
                }
                for (i, param) in func_def.params.iter().enumerate() {
                    if let Some(arg) = args.get(i) {
                        let arg_vals = evaluate(value, arg, ctx)?;
                        let arg_val = arg_vals.into_iter().next().unwrap_or(Value::Null);
                        new_ctx.vars.insert(param.clone(), arg_val);
                    }
                }
                evaluate(value, &func_def.body, &mut new_ctx)
            } else {
                // Try zero-arg version
                let func_key_0 = format!("{}/0", name);
                if let Some(func_def) = ctx.funcs.get(&func_key_0).cloned() {
                    let mut new_ctx = ctx.clone();
                    if let Some(closure) = &func_def.closure {
                        for (k, v) in closure {
                            new_ctx.funcs.insert(k.clone(), v.clone());
                        }
                    }
                    evaluate(value, &func_def.body, &mut new_ctx)
                } else {
                    Err(JqError::Runtime(format!(
                        "Unknown function: {}/{}",
                        name,
                        args.len()
                    )))
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions for builtins
// ---------------------------------------------------------------------------

fn eval_simple_math(value: &Value, name: &str) -> Option<Value> {
    if let Value::Number(n) = value {
        match name {
            "floor" => Some(Value::Number(n.floor())),
            "ceil" => Some(Value::Number(n.ceil())),
            "round" => Some(Value::Number(n.round())),
            "sqrt" => Some(Value::Number(n.sqrt())),
            "exp" => Some(Value::Number(n.exp())),
            "log" | "log_e" => Some(Value::Number(n.ln())),
            "log2" => Some(Value::Number(n.log2())),
            "log10" => Some(Value::Number(n.log10())),
            "sin" => Some(Value::Number(n.sin())),
            "cos" => Some(Value::Number(n.cos())),
            "tan" => Some(Value::Number(n.tan())),
            "asin" => Some(Value::Number(n.asin())),
            "acos" => Some(Value::Number(n.acos())),
            "atan" => Some(Value::Number(n.atan())),
            "sinh" => Some(Value::Number(n.sinh())),
            "cosh" => Some(Value::Number(n.cosh())),
            "tanh" => Some(Value::Number(n.tanh())),
            "asinh" => Some(Value::Number(n.asinh())),
            "acosh" => Some(Value::Number(n.acosh())),
            "atanh" => Some(Value::Number(n.atanh())),
            "cbrt" => Some(Value::Number(n.cbrt())),
            "expm1" => Some(Value::Number(n.exp_m1())),
            "log1p" => Some(Value::Number(n.ln_1p())),
            "trunc" | "truncate" => Some(Value::Number(n.trunc())),
            "rint" => Some(Value::Number(n.round())),
            "j0" | "j1" | "y0" | "y1" => Some(Value::Number(0.0)), // Bessel stubs
            "erf" | "erfc" | "lgamma" | "tgamma" => Some(Value::Number(0.0)), // Special function stubs
            _ => None,
        }
    } else {
        None
    }
}

fn flatten_array(arr: &[Value], depth: i64) -> Vec<Value> {
    let mut result = Vec::new();
    for item in arr {
        if depth > 0 {
            if let Value::Array(inner) = item {
                result.extend(flatten_array(inner, depth - 1));
                continue;
            }
        }
        result.push(item.clone());
    }
    result
}

fn add_values(values: &[Value]) -> Value {
    let filtered: Vec<&Value> = values.iter().filter(|v| !matches!(v, Value::Null)).collect();
    if filtered.is_empty() {
        return Value::Null;
    }
    if filtered.iter().all(|v| matches!(v, Value::Number(_))) {
        let sum: f64 = filtered
            .iter()
            .map(|v| if let Value::Number(n) = v { *n } else { 0.0 })
            .sum();
        return Value::Number(sum);
    }
    if filtered.iter().all(|v| matches!(v, Value::String(_))) {
        let joined: String = filtered
            .iter()
            .map(|v| {
                if let Value::String(s) = v {
                    s.as_str()
                } else {
                    ""
                }
            })
            .collect();
        return Value::String(joined);
    }
    if filtered.iter().all(|v| matches!(v, Value::Array(_))) {
        let mut result = Vec::new();
        for v in &filtered {
            if let Value::Array(arr) = v {
                result.extend(arr.iter().cloned());
            }
        }
        return Value::Array(result);
    }
    if filtered.iter().all(|v| matches!(v, Value::Object(_))) {
        let mut result = IndexMap::new();
        for v in &filtered {
            if let Value::Object(obj) = v {
                for (k, val) in obj {
                    result.insert(k.clone(), val.clone());
                }
            }
        }
        return Value::Object(result);
    }
    Value::Null
}

fn collect_paths(
    value: &Value,
    expr: &AstNode,
    ctx: &mut EvalContext,
    current_path: &[PathElement],
    paths: &mut Vec<Vec<PathElement>>,
) -> Result<(), JqError> {
    match expr {
        AstNode::Identity => {
            paths.push(current_path.to_vec());
        }
        AstNode::Field { name, base } => {
            if let Some(base_expr) = base {
                let base_vals = evaluate(value, base_expr, ctx)?;
                for bv in &base_vals {
                    let mut new_path = current_path.to_vec();
                    // Collect paths from base first
                    let mut base_paths = Vec::new();
                    collect_paths(value, base_expr, ctx, current_path, &mut base_paths)?;
                    for bp in &base_paths {
                        let mut p = bp.clone();
                        p.push(PathElement::Key(name.clone()));
                        paths.push(p);
                    }
                    let _ = bv;
                    let _ = new_path;
                }
            } else {
                let mut new_path = current_path.to_vec();
                new_path.push(PathElement::Key(name.clone()));
                paths.push(new_path);
            }
        }
        AstNode::Index { base, index } => {
            let root_clone = value.clone();
            let indices = evaluate(&root_clone, index, ctx)?;
            if let Some(base_expr) = base {
                let mut base_paths = Vec::new();
                collect_paths(value, base_expr, ctx, current_path, &mut base_paths)?;
                for bp in &base_paths {
                    for idx in &indices {
                        let mut p = bp.clone();
                        match idx {
                            Value::Number(n) => p.push(PathElement::Index(*n as i64)),
                            Value::String(s) => p.push(PathElement::Key(s.clone())),
                            _ => {}
                        }
                        paths.push(p);
                    }
                }
            } else {
                for idx in &indices {
                    let mut new_path = current_path.to_vec();
                    match idx {
                        Value::Number(n) => new_path.push(PathElement::Index(*n as i64)),
                        Value::String(s) => new_path.push(PathElement::Key(s.clone())),
                        _ => {}
                    }
                    paths.push(new_path);
                }
            }
        }
        AstNode::Iterate { base } => {
            let container = if let Some(base_expr) = base {
                evaluate(value, base_expr, ctx)?
                    .into_iter()
                    .next()
                    .unwrap_or(Value::Null)
            } else {
                value.clone()
            };
            let base_paths = if let Some(base_expr) = base {
                let mut bp = Vec::new();
                collect_paths(value, base_expr, ctx, current_path, &mut bp)?;
                bp
            } else {
                vec![current_path.to_vec()]
            };
            for bp in &base_paths {
                match &container {
                    Value::Array(arr) => {
                        for i in 0..arr.len() {
                            let mut p = bp.clone();
                            p.push(PathElement::Index(i as i64));
                            paths.push(p);
                        }
                    }
                    Value::Object(obj) => {
                        for k in obj.keys() {
                            let mut p = bp.clone();
                            p.push(PathElement::Key(k.clone()));
                            paths.push(p);
                        }
                    }
                    _ => {}
                }
            }
        }
        AstNode::Pipe { left, right } => {
            let mut left_paths = Vec::new();
            collect_paths(value, left, ctx, current_path, &mut left_paths)?;
            for lp in &left_paths {
                let mut left_val = value.clone();
                for elem in lp {
                    match (&left_val, elem) {
                        (Value::Array(arr), PathElement::Index(i)) => {
                            left_val = arr.get(*i as usize).cloned().unwrap_or(Value::Null);
                        }
                        (Value::Object(obj), PathElement::Key(k)) => {
                            left_val = obj.get(k).cloned().unwrap_or(Value::Null);
                        }
                        _ => {
                            left_val = Value::Null;
                            break;
                        }
                    }
                }
                collect_paths(&left_val, right, ctx, lp, paths)?;
            }
        }
        AstNode::Comma { left, right } => {
            collect_paths(value, left, ctx, current_path, paths)?;
            collect_paths(value, right, ctx, current_path, paths)?;
        }
        _ => {
            paths.push(current_path.to_vec());
        }
    }
    Ok(())
}

fn collect_all_paths(value: &Value, current: &[PathElement], paths: &mut Vec<Vec<PathElement>>) {
    match value {
        Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                let mut p = current.to_vec();
                p.push(PathElement::Index(i as i64));
                paths.push(p.clone());
                collect_all_paths(item, &p, paths);
            }
        }
        Value::Object(obj) => {
            for (k, v) in obj {
                let mut p = current.to_vec();
                p.push(PathElement::Key(k.clone()));
                paths.push(p.clone());
                collect_all_paths(v, &p, paths);
            }
        }
        _ => {}
    }
}

fn collect_leaf_paths(value: &Value, current: &[PathElement], paths: &mut Vec<Vec<PathElement>>) {
    match value {
        Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                let mut p = current.to_vec();
                p.push(PathElement::Index(i as i64));
                collect_leaf_paths(item, &p, paths);
            }
        }
        Value::Object(obj) => {
            for (k, v) in obj {
                let mut p = current.to_vec();
                p.push(PathElement::Key(k.clone()));
                collect_leaf_paths(v, &p, paths);
            }
        }
        _ => {
            paths.push(current.to_vec());
        }
    }
}

fn recurse_with_filter(
    value: &Value,
    filter: &AstNode,
    cond: Option<&AstNode>,
    ctx: &mut EvalContext,
    results: &mut Vec<Value>,
    depth: usize,
    max_depth: usize,
) -> Result<(), JqError> {
    if depth > max_depth {
        return Ok(());
    }
    if let Some(cond_expr) = cond {
        let cond_results = evaluate(value, cond_expr, ctx)?;
        if !cond_results.iter().any(|v| v.is_truthy()) {
            return Ok(());
        }
    }
    results.push(value.clone());
    let next = evaluate(value, filter, ctx)?;
    for n in &next {
        if !matches!(n, Value::Null) {
            recurse_with_filter(n, filter, cond, ctx, results, depth + 1, max_depth)?;
        }
    }
    Ok(())
}

fn walk_value(value: &Value, expr: &AstNode, ctx: &mut EvalContext) -> Result<Value, JqError> {
    let transformed = match value {
        Value::Array(arr) => {
            let mut new_arr = Vec::new();
            for item in arr {
                new_arr.push(walk_value(item, expr, ctx)?);
            }
            Value::Array(new_arr)
        }
        Value::Object(obj) => {
            let mut new_obj = IndexMap::new();
            for (k, v) in obj {
                new_obj.insert(k.clone(), walk_value(v, expr, ctx)?);
            }
            Value::Object(new_obj)
        }
        _ => value.clone(),
    };
    let results = evaluate(&transformed, expr, ctx)?;
    Ok(results.into_iter().next().unwrap_or(Value::Null))
}

fn generate_combinations(arr: &[Value], n: usize, current: &mut Vec<Value>, results: &mut Vec<Value>) {
    if current.len() == n {
        results.push(Value::Array(current.clone()));
        return;
    }
    for item in arr {
        current.push(item.clone());
        generate_combinations(arr, n, current, results);
        current.pop();
    }
}

fn generate_cartesian(arrays: &[Value], index: usize, current: &mut Vec<Value>, results: &mut Vec<Value>) {
    if index == arrays.len() {
        results.push(Value::Array(current.clone()));
        return;
    }
    if let Value::Array(arr) = &arrays[index] {
        for item in arr {
            current.push(item.clone());
            generate_cartesian(arrays, index + 1, current, results);
            current.pop();
        }
    }
}

fn collect_stream_pairs(value: &Value, path: &[Value], results: &mut Vec<Value>) {
    match value {
        Value::Array(arr) if !arr.is_empty() => {
            for (i, item) in arr.iter().enumerate() {
                let mut new_path = path.to_vec();
                new_path.push(Value::Number(i as f64));
                collect_stream_pairs(item, &new_path, results);
            }
        }
        Value::Object(obj) if !obj.is_empty() => {
            for (k, v) in obj {
                let mut new_path = path.to_vec();
                new_path.push(Value::String(k.clone()));
                collect_stream_pairs(v, &new_path, results);
            }
        }
        _ => {
            results.push(Value::Array(vec![
                Value::Array(path.to_vec()),
                value.clone(),
            ]));
        }
    }
}

fn percent_encode(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

fn format_number(n: f64) -> String {
    if n == n.trunc() && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

fn get_builtin_names() -> Vec<&'static str> {
    vec![
        "length", "utf8bytelength", "keys", "keys_unsorted", "values", "has", "in",
        "contains", "inside", "type", "infinite", "nan", "isinfinite", "isnan",
        "isnormal", "isfinite", "sort", "sort_by", "group_by", "unique", "unique_by",
        "max", "max_by", "min", "min_by", "add", "any", "all", "flatten", "range",
        "floor", "ceil", "round", "sqrt", "pow", "fabs", "abs", "exp", "log",
        "log2", "log10", "sin", "cos", "tan", "asin", "acos", "atan", "atan2",
        "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "cbrt", "exp2", "exp10",
        "expm1", "log1p", "hypot", "fma", "copysign", "drem", "remainder", "fdim",
        "fmax", "fmin", "ldexp", "scalbn", "scalbln", "nearbyint", "logb",
        "significand", "frexp", "modf", "trunc", "truncate", "rint",
        "tostring", "tonumber", "toboolean", "tojson", "fromjson",
        "ascii_downcase", "ascii_upcase", "ltrimstr", "rtrimstr", "startswith",
        "endswith", "split", "join", "test", "match", "capture", "scan", "sub",
        "gsub", "ascii", "explode", "implode", "trim", "ltrim", "rtrim",
        "splits", "to_entries", "from_entries", "with_entries", "keys",
        "values", "has", "in", "map", "map_values", "select", "empty",
        "error", "debug", "not", "null", "true", "false",
        "first", "last", "nth", "range", "limit", "isempty", "isvalid",
        "skip", "until", "while", "repeat",
        "recurse", "recurse_down", "walk", "transpose", "combinations",
        "parent", "parents", "root",
        "getpath", "setpath", "delpaths", "path", "del", "pick", "paths",
        "leaf_paths",
        "index", "rindex", "indices",
        "reverse", "flatten", "sort", "unique",
        "bsearch",
        "@base64", "@base64d", "@uri", "@csv", "@tsv", "@json", "@html", "@sh", "@text",
        "now", "gmtime", "mktime", "strftime", "todate", "fromdate",
        "IN", "INDEX", "JOIN",
        "builtins", "input_line_number",
        "numbers", "strings", "booleans", "nulls", "arrays", "objects",
        "iterables", "scalars",
        "tostream", "fromstream",
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::query_engine::parser::parse;

    fn eval(input: &str, query: &str) -> Vec<Value> {
        let value: serde_json::Value = serde_json::from_str(input).unwrap();
        let val = Value::from_serde_json(value);
        let ast = parse(query).unwrap();
        let mut ctx = EvalContext::new();
        evaluate(&val, &ast, &mut ctx).unwrap()
    }

    fn eval_first(input: &str, query: &str) -> Value {
        eval(input, query).into_iter().next().unwrap()
    }

    fn eval_err(input: &str, query: &str) -> bool {
        let value: serde_json::Value = serde_json::from_str(input).unwrap();
        let val = Value::from_serde_json(value);
        let ast = parse(query).unwrap();
        let mut ctx = EvalContext::new();
        evaluate(&val, &ast, &mut ctx).is_err()
    }

    #[test]
    fn test_identity() {
        assert_eq!(eval_first(r#"42"#, "."), Value::Number(42.0));
        assert_eq!(eval_first(r#""hello""#, "."), Value::String("hello".to_string()));
        assert_eq!(eval_first("null", "."), Value::Null);
    }

    #[test]
    fn test_field() {
        assert_eq!(
            eval_first(r#"{"name":"Alice","age":30}"#, ".name"),
            Value::String("Alice".to_string())
        );
        assert_eq!(
            eval_first(r#"{"name":"Alice","age":30}"#, ".age"),
            Value::Number(30.0)
        );
        assert_eq!(
            eval_first(r#"{"a":{"b":1}}"#, ".a.b"),
            Value::Number(1.0)
        );
    }

    #[test]
    fn test_index() {
        assert_eq!(
            eval_first(r#"[10,20,30]"#, ".[1]"),
            Value::Number(20.0)
        );
        assert_eq!(
            eval_first(r#"[10,20,30]"#, ".[-1]"),
            Value::Number(30.0)
        );
    }

    #[test]
    fn test_slice() {
        assert_eq!(
            eval_first(r#"[0,1,2,3,4]"#, ".[2:4]"),
            Value::Array(vec![Value::Number(2.0), Value::Number(3.0)])
        );
        assert_eq!(
            eval_first(r#""hello""#, ".[1:3]"),
            Value::String("el".to_string())
        );
    }

    #[test]
    fn test_iterate() {
        let result = eval(r#"[1,2,3]"#, ".[]");
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], Value::Number(1.0));
        assert_eq!(result[2], Value::Number(3.0));
    }

    #[test]
    fn test_pipe() {
        assert_eq!(
            eval_first(r#"{"a":{"b":1}}"#, ".a | .b"),
            Value::Number(1.0)
        );
    }

    #[test]
    fn test_comma() {
        let result = eval(r#"{"a":1,"b":2}"#, ".a, .b");
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], Value::Number(1.0));
        assert_eq!(result[1], Value::Number(2.0));
    }

    #[test]
    fn test_literal() {
        assert_eq!(eval_first("null", "42"), Value::Number(42.0));
        assert_eq!(
            eval_first("null", r#""hello""#),
            Value::String("hello".to_string())
        );
        assert_eq!(eval_first("null", "true"), Value::Bool(true));
        assert_eq!(eval_first("null", "null"), Value::Null);
    }

    #[test]
    fn test_array_construction() {
        assert_eq!(
            eval_first(r#"{"a":1,"b":2}"#, "[.a, .b]"),
            Value::Array(vec![Value::Number(1.0), Value::Number(2.0)])
        );
        assert_eq!(eval_first("null", "[]"), Value::Array(vec![]));
    }

    #[test]
    fn test_object_construction() {
        let result = eval_first(r#"{"x":1}"#, r#"{a: .x, b: "hello"}"#);
        if let Value::Object(obj) = result {
            assert_eq!(obj.get("a"), Some(&Value::Number(1.0)));
            assert_eq!(obj.get("b"), Some(&Value::String("hello".to_string())));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_binary_ops() {
        assert_eq!(eval_first("null", "1 + 2"), Value::Number(3.0));
        assert_eq!(eval_first("null", "10 - 3"), Value::Number(7.0));
        assert_eq!(eval_first("null", "3 * 4"), Value::Number(12.0));
        assert_eq!(eval_first("null", "10 / 2"), Value::Number(5.0));
        assert_eq!(eval_first("null", "10 % 3"), Value::Number(1.0));
        assert_eq!(eval_first("null", "1 == 1"), Value::Bool(true));
        assert_eq!(eval_first("null", "1 != 2"), Value::Bool(true));
        assert_eq!(eval_first("null", "1 < 2"), Value::Bool(true));
        assert_eq!(eval_first("null", "2 > 1"), Value::Bool(true));
        assert_eq!(eval_first("null", "1 <= 1"), Value::Bool(true));
        assert_eq!(eval_first("null", "1 >= 1"), Value::Bool(true));
    }

    #[test]
    fn test_string_concat() {
        assert_eq!(
            eval_first("null", r#""hello" + " " + "world""#),
            Value::String("hello world".to_string())
        );
    }

    #[test]
    fn test_array_concat() {
        assert_eq!(
            eval_first("null", "[1,2] + [3,4]"),
            Value::Array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Number(4.0),
            ])
        );
    }

    #[test]
    fn test_and_or() {
        assert_eq!(eval_first("null", "true and true"), Value::Bool(true));
        assert_eq!(eval_first("null", "true and false"), Value::Bool(false));
        assert_eq!(eval_first("null", "false or true"), Value::Bool(true));
        assert_eq!(eval_first("null", "false or false"), Value::Bool(false));
    }

    #[test]
    fn test_alternative() {
        assert_eq!(eval_first("null", "null // 42"), Value::Number(42.0));
        assert_eq!(eval_first("null", "1 // 42"), Value::Number(1.0));
    }

    #[test]
    fn test_unary_neg() {
        assert_eq!(eval_first("null", "-(3)"), Value::Number(-3.0));
    }

    #[test]
    fn test_not() {
        assert_eq!(eval_first("null", "true | not"), Value::Bool(false));
        assert_eq!(eval_first("null", "false | not"), Value::Bool(true));
        assert_eq!(eval_first("null", "null | not"), Value::Bool(true));
    }

    #[test]
    fn test_cond() {
        assert_eq!(
            eval_first("null", "if true then 1 else 2 end"),
            Value::Number(1.0)
        );
        assert_eq!(
            eval_first("null", "if false then 1 else 2 end"),
            Value::Number(2.0)
        );
    }

    #[test]
    fn test_try_catch() {
        assert_eq!(
            eval_first("null", "try error catch ."),
            Value::Null
        );
    }

    #[test]
    fn test_optional() {
        let result = eval("null", ".foo?");
        assert!(result.is_empty() || result == vec![Value::Null]);
    }

    #[test]
    fn test_var_bind() {
        assert_eq!(
            eval_first("null", "1 as $x | $x + 2"),
            Value::Number(3.0)
        );
    }

    #[test]
    fn test_var_ref() {
        assert_eq!(
            eval_first(r#"{"a":5}"#, ".a as $x | $x * $x"),
            Value::Number(25.0)
        );
    }

    #[test]
    fn test_recurse() {
        let result = eval(r#"{"a":{"b":1}}"#, ".. | numbers");
        assert_eq!(result, vec![Value::Number(1.0)]);
    }

    #[test]
    fn test_string_interp() {
        assert_eq!(
            eval_first(r#"{"name":"world"}"#, r#""hello \(.name)""#),
            Value::String("hello world".to_string())
        );
    }

    #[test]
    fn test_update_op() {
        assert_eq!(
            eval_first(r#"{"a":1}"#, ".a = 2"),
            Value::Object({
                let mut m = IndexMap::new();
                m.insert("a".to_string(), Value::Number(2.0));
                m
            })
        );
    }

    #[test]
    fn test_update_pipe() {
        assert_eq!(
            eval_first(r#"{"a":1}"#, ".a |= . + 1"),
            Value::Object({
                let mut m = IndexMap::new();
                m.insert("a".to_string(), Value::Number(2.0));
                m
            })
        );
    }

    #[test]
    fn test_reduce() {
        assert_eq!(
            eval_first("null", "reduce range(5) as $x (0; . + $x)"),
            Value::Number(10.0)
        );
    }

    #[test]
    fn test_foreach() {
        let result = eval("null", "foreach range(3) as $x (0; . + $x)");
        assert_eq!(result, vec![
            Value::Number(0.0),
            Value::Number(1.0),
            Value::Number(3.0),
        ]);
    }

    #[test]
    fn test_label_break() {
        // Test that label/break mechanism works - break exits the label scope
        let result = eval("null", "label $out | 1, 2, (break $out), 3");
        // After break, only values before break are returned
        assert_eq!(result, vec![Value::Number(1.0), Value::Number(2.0)]);
    }

    #[test]
    fn test_def() {
        assert_eq!(
            eval_first("null", "def double: . * 2; 5 | double"),
            Value::Number(10.0)
        );
    }

    #[test]
    fn test_length() {
        assert_eq!(eval_first(r#""hello""#, "length"), Value::Number(5.0));
        assert_eq!(eval_first("[1,2,3]", "length"), Value::Number(3.0));
        assert_eq!(eval_first(r#"{"a":1,"b":2}"#, "length"), Value::Number(2.0));
        assert_eq!(eval_first("null", "null | length"), Value::Number(0.0));
    }

    #[test]
    fn test_keys() {
        let result = eval_first(r#"{"b":2,"a":1}"#, "keys");
        if let Value::Array(arr) = result {
            assert_eq!(arr[0], Value::String("a".to_string()));
            assert_eq!(arr[1], Value::String("b".to_string()));
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_values_builtin() {
        let result = eval(r#"{"a":1,"b":2}"#, "[.[] | values]");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_has() {
        assert_eq!(
            eval_first(r#"{"a":1}"#, r#"has("a")"#),
            Value::Bool(true)
        );
        assert_eq!(
            eval_first(r#"{"a":1}"#, r#"has("b")"#),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_select() {
        let result = eval("[1,2,3,4,5]", "[.[] | select(. > 3)]");
        assert_eq!(
            result[0],
            Value::Array(vec![Value::Number(4.0), Value::Number(5.0)])
        );
    }

    #[test]
    fn test_map() {
        assert_eq!(
            eval_first("[1,2,3]", "map(. * 2)"),
            Value::Array(vec![
                Value::Number(2.0),
                Value::Number(4.0),
                Value::Number(6.0),
            ])
        );
    }

    #[test]
    fn test_sort() {
        assert_eq!(
            eval_first("[3,1,2]", "sort"),
            Value::Array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn test_reverse() {
        assert_eq!(
            eval_first("[1,2,3]", "reverse"),
            Value::Array(vec![
                Value::Number(3.0),
                Value::Number(2.0),
                Value::Number(1.0),
            ])
        );
    }

    #[test]
    fn test_flatten() {
        assert_eq!(
            eval_first("[[1,2],[3,[4]]]", "flatten"),
            Value::Array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Number(4.0),
            ])
        );
    }

    #[test]
    fn test_unique() {
        assert_eq!(
            eval_first("[1,2,1,3,2]", "unique"),
            Value::Array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn test_add() {
        assert_eq!(eval_first("[1,2,3]", "add"), Value::Number(6.0));
        assert_eq!(
            eval_first(r#"["a","b","c"]"#, "add"),
            Value::String("abc".to_string())
        );
    }

    #[test]
    fn test_type() {
        assert_eq!(eval_first("null", "null | type"), Value::String("null".to_string()));
        assert_eq!(eval_first("null", "1 | type"), Value::String("number".to_string()));
        assert_eq!(eval_first("null", r#""s" | type"#), Value::String("string".to_string()));
        assert_eq!(eval_first("null", "true | type"), Value::String("boolean".to_string()));
        assert_eq!(eval_first("null", "[] | type"), Value::String("array".to_string()));
        assert_eq!(eval_first("null", "{} | type"), Value::String("object".to_string()));
    }

    #[test]
    fn test_empty() {
        let result = eval("null", "empty");
        assert!(result.is_empty());
    }

    #[test]
    fn test_error() {
        assert!(eval_err("null", "error"));
    }

    #[test]
    fn test_range() {
        let result = eval("null", "[range(5)]");
        assert_eq!(
            result[0],
            Value::Array(vec![
                Value::Number(0.0),
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Number(4.0),
            ])
        );
    }

    #[test]
    fn test_first_last() {
        assert_eq!(eval_first("null", "first(range(5))"), Value::Number(0.0));
        assert_eq!(eval_first("null", "last(range(5))"), Value::Number(4.0));
    }

    #[test]
    fn test_join() {
        assert_eq!(
            eval_first(r#"["a","b","c"]"#, r#"join(",")"#),
            Value::String("a,b,c".to_string())
        );
    }

    #[test]
    fn test_split() {
        assert_eq!(
            eval_first(r#""a,b,c""#, r#"split(",")"#),
            Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
                Value::String("c".to_string()),
            ])
        );
    }

    #[test]
    fn test_test() {
        assert_eq!(
            eval_first(r#""foobar""#, r#"test("foo")"#),
            Value::Bool(true)
        );
        assert_eq!(
            eval_first(r#""foobar""#, r#"test("^bar")"#),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_gsub() {
        assert_eq!(
            eval_first(r#""hello world""#, r#"gsub("o"; "0")"#),
            Value::String("hell0 w0rld".to_string())
        );
    }

    #[test]
    fn test_getpath() {
        assert_eq!(
            eval_first(r#"{"a":{"b":1}}"#, r#"getpath(["a","b"])"#),
            Value::Number(1.0)
        );
    }

    #[test]
    fn test_setpath() {
        let result = eval_first(r#"{"a":1}"#, r#"setpath(["b"]; 2)"#);
        if let Value::Object(obj) = result {
            assert_eq!(obj.get("b"), Some(&Value::Number(2.0)));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_del() {
        let result = eval_first(r#"{"a":1,"b":2}"#, "del(.a)");
        if let Value::Object(obj) = result {
            assert!(!obj.contains_key("a"));
            assert!(obj.contains_key("b"));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_paths() {
        let result = eval(r#"{"a":1,"b":{"c":2}}"#, "[paths]");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_base64() {
        assert_eq!(
            eval_first(r#""hello""#, "@base64"),
            Value::String("aGVsbG8=".to_string())
        );
        assert_eq!(
            eval_first(r#""aGVsbG8=""#, "@base64d"),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_csv() {
        assert_eq!(
            eval_first(r#"[1,"two",3]"#, "@csv"),
            Value::String("1,two,3".to_string())
        );
    }

    #[test]
    fn test_json_format() {
        assert_eq!(
            eval_first(r#"{"a":1}"#, "@json"),
            Value::String(r#"{"a":1}"#.to_string())
        );
    }

    #[test]
    fn test_now() {
        let result = eval_first("null", "now");
        if let Value::Number(n) = result {
            assert!(n > 1000000000.0);
        } else {
            panic!("Expected number");
        }
    }

    #[test]
    fn test_todate() {
        assert_eq!(
            eval_first("0", "todate"),
            Value::String("1970-01-01T00:00:00Z".to_string())
        );
    }

    #[test]
    fn test_recurse_builtin() {
        let result = eval(r#"{"a":{"b":1}}"#, "[recurse | .b? // empty]");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_walk() {
        assert_eq!(
            eval_first("[1,2,3]", "walk(if type == \"number\" then . * 2 else . end)"),
            Value::Array(vec![
                Value::Number(2.0),
                Value::Number(4.0),
                Value::Number(6.0),
            ])
        );
    }

    #[test]
    fn test_in_builtin() {
        assert_eq!(
            eval_first("null", r#""a" | IN("a", "b", "c")"#),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_contains() {
        assert_eq!(
            eval_first(r#"[1,2,3]"#, "contains([2,3])"),
            Value::Bool(true)
        );
        assert_eq!(
            eval_first(r#"[1,2,3]"#, "contains([4])"),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_math_builtins() {
        assert_eq!(eval_first("null", "2.7 | floor"), Value::Number(2.0));
        assert_eq!(eval_first("null", "2.1 | ceil"), Value::Number(3.0));
        assert_eq!(eval_first("null", "2.5 | round"), Value::Number(3.0));
        let sqrt_result = eval_first("null", "4 | sqrt");
        if let Value::Number(n) = sqrt_result {
            assert!((n - 2.0).abs() < 0.0001);
        }
    }

    #[test]
    fn test_to_entries_from_entries() {
        let result = eval_first(r#"{"a":1}"#, "to_entries");
        if let Value::Array(arr) = &result {
            assert_eq!(arr.len(), 1);
            if let Value::Object(entry) = &arr[0] {
                assert_eq!(entry.get("key"), Some(&Value::String("a".to_string())));
                assert_eq!(entry.get("value"), Some(&Value::Number(1.0)));
            }
        }

        let result2 = eval_first(r#"{"a":1}"#, "to_entries | from_entries");
        if let Value::Object(obj) = result2 {
            assert_eq!(obj.get("a"), Some(&Value::Number(1.0)));
        }
    }

    #[test]
    fn test_ascii_case() {
        assert_eq!(
            eval_first(r#""Hello""#, "ascii_downcase"),
            Value::String("hello".to_string())
        );
        assert_eq!(
            eval_first(r#""Hello""#, "ascii_upcase"),
            Value::String("HELLO".to_string())
        );
    }

    #[test]
    fn test_tojson_fromjson() {
        assert_eq!(
            eval_first(r#"{"a":1}"#, "tojson"),
            Value::String(r#"{"a":1}"#.to_string())
        );
        assert_eq!(
            eval_first(r#""[1,2,3]""#, "fromjson"),
            Value::Array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
            ])
        );
    }

    #[test]
    fn test_div_by_zero() {
        assert!(eval_err("null", "1 / 0"));
    }

    #[test]
    fn test_null_addition() {
        assert_eq!(eval_first("null", "null + 1"), Value::Number(1.0));
        assert_eq!(eval_first("null", "1 + null"), Value::Number(1.0));
    }

    #[test]
    fn test_object_merge() {
        let result = eval_first("null", r#"{"a":1} + {"b":2}"#);
        if let Value::Object(obj) = result {
            assert_eq!(obj.get("a"), Some(&Value::Number(1.0)));
            assert_eq!(obj.get("b"), Some(&Value::Number(2.0)));
        }
    }

    #[test]
    fn test_array_subtraction() {
        assert_eq!(
            eval_first("null", "[1,2,3,2,1] - [1,2]"),
            Value::Array(vec![Value::Number(3.0)])
        );
    }

    #[test]
    fn test_string_split_by_div() {
        assert_eq!(
            eval_first("null", r#""a,b,c" / ",""#),
            Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
                Value::String("c".to_string()),
            ])
        );
    }

    #[test]
    fn test_comparison_ops() {
        assert_eq!(eval_first("null", r#""a" < "b""#), Value::Bool(true));
        assert_eq!(eval_first("null", "null == null"), Value::Bool(true));
        assert_eq!(eval_first("null", "null < false"), Value::Bool(true));
    }
}
