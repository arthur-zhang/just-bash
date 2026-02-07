use std::cmp::Ordering;
use indexmap::IndexMap;
use super::value::Value;
use super::context::PathElement;

/// jq truthiness: false and null are falsy
pub fn is_truthy(v: &Value) -> bool {
    v.is_truthy()
}

/// Deep equality check
pub fn deep_equal(a: &Value, b: &Value) -> bool {
    a == b
}

/// Type ordering for jq: null < bool < number < string < array < object
fn type_order(v: &Value) -> u8 {
    match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Object(_) => 5,
    }
}

/// Compare two values of the same type for sorting
pub fn compare(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (Value::Number(a), Value::Number(b)) => {
            a.partial_cmp(b).unwrap_or(Ordering::Equal)
        }
        (Value::String(a), Value::String(b)) => a.cmp(b),
        (Value::Array(a), Value::Array(b)) => {
            for (x, y) in a.iter().zip(b.iter()) {
                let c = compare(x, y);
                if c != Ordering::Equal {
                    return c;
                }
            }
            a.len().cmp(&b.len())
        }
        _ => Ordering::Equal,
    }
}

/// jq comparison with type ordering: null < bool < number < string < array < object
pub fn compare_jq(a: &Value, b: &Value) -> Ordering {
    let ta = type_order(a);
    let tb = type_order(b);
    if ta != tb {
        return ta.cmp(&tb);
    }
    compare(a, b)
}

/// Deep merge two objects. For non-objects, b wins.
pub fn deep_merge(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Object(obj_a), Value::Object(obj_b)) => {
            let mut result = obj_a.clone();
            for (key, val_b) in obj_b {
                if let Some(val_a) = obj_a.get(key) {
                    result.insert(key.clone(), deep_merge(val_a, val_b));
                } else {
                    result.insert(key.clone(), val_b.clone());
                }
            }
            Value::Object(result)
        }
        _ => b.clone(),
    }
}

/// jq containment semantics: a contains b
pub fn contains_deep(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(obj_a), Value::Object(obj_b)) => {
            for (key, val_b) in obj_b {
                match obj_a.get(key) {
                    Some(val_a) => {
                        if !contains_deep(val_a, val_b) {
                            return false;
                        }
                    }
                    None => return false,
                }
            }
            true
        }
        (Value::Array(arr_a), Value::Array(arr_b)) => {
            for val_b in arr_b {
                let found = arr_a.iter().any(|val_a| contains_deep(val_a, val_b));
                if !found {
                    return false;
                }
            }
            true
        }
        (Value::String(sa), Value::String(sb)) => sa.contains(sb.as_str()),
        _ => a == b,
    }
}

/// Calculate nesting depth of a value
pub fn get_value_depth(v: &Value, max_check: usize) -> usize {
    if max_check == 0 {
        return 0;
    }
    match v {
        Value::Array(arr) => {
            let max_child = arr
                .iter()
                .map(|item| get_value_depth(item, max_check - 1))
                .max()
                .unwrap_or(0);
            1 + max_child
        }
        Value::Object(obj) => {
            let max_child = obj
                .values()
                .map(|val| get_value_depth(val, max_check - 1))
                .max()
                .unwrap_or(0);
            1 + max_child
        }
        _ => 0,
    }
}

/// Set value at path, creating intermediates as needed
pub fn set_path(value: &Value, path: &[PathElement], new_val: Value) -> Value {
    if path.is_empty() {
        return new_val;
    }

    let first = &path[0];
    let rest = &path[1..];

    match first {
        PathElement::Key(key) => {
            let mut obj = match value {
                Value::Object(o) => o.clone(),
                _ => IndexMap::new(),
            };
            let existing = obj.get(key).cloned().unwrap_or(Value::Null);
            obj.insert(key.clone(), set_path(&existing, rest, new_val));
            Value::Object(obj)
        }
        PathElement::Index(idx) => {
            let mut arr = match value {
                Value::Array(a) => a.clone(),
                _ => Vec::new(),
            };
            let index = if *idx < 0 {
                let len = arr.len() as i64;
                ((len + idx).max(0)) as usize
            } else {
                *idx as usize
            };
            // Extend array if needed
            while arr.len() <= index {
                arr.push(Value::Null);
            }
            let existing = arr[index].clone();
            arr[index] = set_path(&existing, rest, new_val);
            Value::Array(arr)
        }
    }
}

/// Delete value at path
pub fn delete_path(value: &Value, path: &[PathElement]) -> Value {
    if path.is_empty() {
        return Value::Null;
    }

    if path.len() == 1 {
        match &path[0] {
            PathElement::Key(key) => {
                if let Value::Object(obj) = value {
                    let mut new_obj = obj.clone();
                    new_obj.shift_remove(key);
                    return Value::Object(new_obj);
                }
                return value.clone();
            }
            PathElement::Index(idx) => {
                if let Value::Array(arr) = value {
                    let index = if *idx < 0 {
                        let len = arr.len() as i64;
                        (len + idx) as usize
                    } else {
                        *idx as usize
                    };
                    if index < arr.len() {
                        let mut new_arr = arr.clone();
                        new_arr.remove(index);
                        return Value::Array(new_arr);
                    }
                }
                return value.clone();
            }
        }
    }

    let first = &path[0];
    let rest = &path[1..];

    match first {
        PathElement::Key(key) => {
            if let Value::Object(obj) = value {
                let mut new_obj = obj.clone();
                if let Some(child) = obj.get(key) {
                    new_obj.insert(key.clone(), delete_path(child, rest));
                }
                return Value::Object(new_obj);
            }
            value.clone()
        }
        PathElement::Index(idx) => {
            if let Value::Array(arr) = value {
                let index = if *idx < 0 {
                    let len = arr.len() as i64;
                    (len + idx) as usize
                } else {
                    *idx as usize
                };
                if index < arr.len() {
                    let mut new_arr = arr.clone();
                    new_arr[index] = delete_path(&arr[index], rest);
                    return Value::Array(new_arr);
                }
            }
            value.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create an object Value
    fn obj(pairs: Vec<(&str, Value)>) -> Value {
        let mut map = IndexMap::new();
        for (k, v) in pairs {
            map.insert(k.to_string(), v);
        }
        Value::Object(map)
    }

    #[test]
    fn test_is_truthy() {
        assert!(!is_truthy(&Value::Null));
        assert!(!is_truthy(&Value::Bool(false)));
        assert!(is_truthy(&Value::Bool(true)));
        assert!(is_truthy(&Value::Number(0.0)));
        assert!(is_truthy(&Value::Number(1.0)));
        assert!(is_truthy(&Value::String("".to_string())));
        assert!(is_truthy(&Value::String("hello".to_string())));
        assert!(is_truthy(&Value::Array(vec![])));
        assert!(is_truthy(&obj(vec![])));
    }

    #[test]
    fn test_deep_equal_primitives() {
        assert!(deep_equal(&Value::Null, &Value::Null));
        assert!(deep_equal(&Value::Bool(true), &Value::Bool(true)));
        assert!(!deep_equal(&Value::Bool(true), &Value::Bool(false)));
        assert!(deep_equal(&Value::Number(42.0), &Value::Number(42.0)));
        assert!(!deep_equal(&Value::Number(1.0), &Value::Number(2.0)));
        assert!(deep_equal(
            &Value::String("abc".to_string()),
            &Value::String("abc".to_string())
        ));
        assert!(!deep_equal(&Value::Null, &Value::Bool(false)));
    }

    #[test]
    fn test_deep_equal_nan() {
        // NaN != NaN per jq semantics
        assert!(!deep_equal(&Value::Number(f64::NAN), &Value::Number(f64::NAN)));
    }

    #[test]
    fn test_deep_equal_objects() {
        let a = obj(vec![("x", Value::Number(1.0)), ("y", Value::Number(2.0))]);
        let b = obj(vec![("y", Value::Number(2.0)), ("x", Value::Number(1.0))]);
        // Objects with same keys/values but different order should be equal
        assert!(deep_equal(&a, &b));
    }

    #[test]
    fn test_deep_equal_arrays() {
        let a = Value::Array(vec![Value::Number(1.0), Value::Number(2.0)]);
        let b = Value::Array(vec![Value::Number(1.0), Value::Number(2.0)]);
        assert!(deep_equal(&a, &b));

        let c = Value::Array(vec![Value::Number(2.0), Value::Number(1.0)]);
        assert!(!deep_equal(&a, &c));
    }

    #[test]
    fn test_compare_jq_type_ordering() {
        // null < bool < number < string < array < object
        assert_eq!(
            compare_jq(&Value::Null, &Value::Bool(false)),
            Ordering::Less
        );
        assert_eq!(
            compare_jq(&Value::Bool(true), &Value::Number(0.0)),
            Ordering::Less
        );
        assert_eq!(
            compare_jq(&Value::Number(999.0), &Value::String("a".to_string())),
            Ordering::Less
        );
        assert_eq!(
            compare_jq(&Value::String("z".to_string()), &Value::Array(vec![])),
            Ordering::Less
        );
        assert_eq!(
            compare_jq(&Value::Array(vec![]), &obj(vec![])),
            Ordering::Less
        );
    }

    #[test]
    fn test_compare_jq_same_type() {
        assert_eq!(
            compare_jq(&Value::Number(1.0), &Value::Number(2.0)),
            Ordering::Less
        );
        assert_eq!(
            compare_jq(&Value::Number(2.0), &Value::Number(1.0)),
            Ordering::Greater
        );
        assert_eq!(
            compare_jq(&Value::Number(1.0), &Value::Number(1.0)),
            Ordering::Equal
        );
        assert_eq!(
            compare_jq(
                &Value::String("abc".to_string()),
                &Value::String("def".to_string())
            ),
            Ordering::Less
        );
    }

    #[test]
    fn test_deep_merge_objects() {
        let a = obj(vec![
            ("x", Value::Number(1.0)),
            ("nested", obj(vec![("a", Value::Number(10.0))])),
        ]);
        let b = obj(vec![
            ("y", Value::Number(2.0)),
            ("nested", obj(vec![("b", Value::Number(20.0))])),
        ]);
        let merged = deep_merge(&a, &b);
        let expected = obj(vec![
            ("x", Value::Number(1.0)),
            (
                "nested",
                obj(vec![
                    ("a", Value::Number(10.0)),
                    ("b", Value::Number(20.0)),
                ]),
            ),
            ("y", Value::Number(2.0)),
        ]);
        assert!(deep_equal(&merged, &expected));
    }

    #[test]
    fn test_deep_merge_non_objects() {
        // Non-object merge: b wins
        let result = deep_merge(&Value::Number(1.0), &Value::Number(2.0));
        assert_eq!(result, Value::Number(2.0));
    }

    #[test]
    fn test_contains_deep_primitives() {
        assert!(contains_deep(&Value::Number(1.0), &Value::Number(1.0)));
        assert!(!contains_deep(&Value::Number(1.0), &Value::Number(2.0)));
        assert!(contains_deep(
            &Value::String("hello world".to_string()),
            &Value::String("hello".to_string())
        ));
        assert!(!contains_deep(
            &Value::String("hello".to_string()),
            &Value::String("world".to_string())
        ));
    }

    #[test]
    fn test_contains_deep_objects() {
        let a = obj(vec![
            ("x", Value::Number(1.0)),
            ("y", Value::Number(2.0)),
            ("z", Value::Number(3.0)),
        ]);
        let b = obj(vec![("x", Value::Number(1.0))]);
        assert!(contains_deep(&a, &b));

        let c = obj(vec![("w", Value::Number(4.0))]);
        assert!(!contains_deep(&a, &c));
    }

    #[test]
    fn test_contains_deep_arrays() {
        let a = Value::Array(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ]);
        let b = Value::Array(vec![Value::Number(1.0), Value::Number(3.0)]);
        assert!(contains_deep(&a, &b));

        let c = Value::Array(vec![Value::Number(4.0)]);
        assert!(!contains_deep(&a, &c));
    }

    #[test]
    fn test_set_path_object() {
        let val = obj(vec![("a", Value::Number(1.0))]);
        let result = set_path(
            &val,
            &[PathElement::Key("b".to_string())],
            Value::Number(2.0),
        );
        let expected = obj(vec![
            ("a", Value::Number(1.0)),
            ("b", Value::Number(2.0)),
        ]);
        assert!(deep_equal(&result, &expected));
    }

    #[test]
    fn test_set_path_nested() {
        let val = obj(vec![("a", obj(vec![("b", Value::Number(1.0))]))]);
        let result = set_path(
            &val,
            &[
                PathElement::Key("a".to_string()),
                PathElement::Key("c".to_string()),
            ],
            Value::Number(2.0),
        );
        let expected = obj(vec![(
            "a",
            obj(vec![
                ("b", Value::Number(1.0)),
                ("c", Value::Number(2.0)),
            ]),
        )]);
        assert!(deep_equal(&result, &expected));
    }

    #[test]
    fn test_set_path_array() {
        let val = Value::Array(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ]);
        let result = set_path(&val, &[PathElement::Index(1)], Value::Number(99.0));
        let expected = Value::Array(vec![
            Value::Number(1.0),
            Value::Number(99.0),
            Value::Number(3.0),
        ]);
        assert!(deep_equal(&result, &expected));
    }

    #[test]
    fn test_set_path_extend_array() {
        let val = Value::Array(vec![Value::Number(1.0)]);
        let result = set_path(&val, &[PathElement::Index(3)], Value::Number(4.0));
        let expected = Value::Array(vec![
            Value::Number(1.0),
            Value::Null,
            Value::Null,
            Value::Number(4.0),
        ]);
        assert!(deep_equal(&result, &expected));
    }

    #[test]
    fn test_delete_path_object() {
        let val = obj(vec![
            ("a", Value::Number(1.0)),
            ("b", Value::Number(2.0)),
        ]);
        let result = delete_path(&val, &[PathElement::Key("a".to_string())]);
        let expected = obj(vec![("b", Value::Number(2.0))]);
        assert!(deep_equal(&result, &expected));
    }

    #[test]
    fn test_delete_path_array() {
        let val = Value::Array(vec![
            Value::Number(1.0),
            Value::Number(2.0),
            Value::Number(3.0),
        ]);
        let result = delete_path(&val, &[PathElement::Index(1)]);
        let expected = Value::Array(vec![Value::Number(1.0), Value::Number(3.0)]);
        assert!(deep_equal(&result, &expected));
    }

    #[test]
    fn test_delete_path_nested() {
        let val = obj(vec![(
            "a",
            obj(vec![
                ("b", Value::Number(1.0)),
                ("c", Value::Number(2.0)),
            ]),
        )]);
        let result = delete_path(
            &val,
            &[
                PathElement::Key("a".to_string()),
                PathElement::Key("b".to_string()),
            ],
        );
        let expected = obj(vec![("a", obj(vec![("c", Value::Number(2.0))]))]);
        assert!(deep_equal(&result, &expected));
    }

    #[test]
    fn test_get_value_depth() {
        assert_eq!(get_value_depth(&Value::Number(1.0), 100), 0);
        assert_eq!(
            get_value_depth(&Value::Array(vec![Value::Number(1.0)]), 100),
            1
        );
        let nested = Value::Array(vec![Value::Array(vec![Value::Number(1.0)])]);
        assert_eq!(get_value_depth(&nested, 100), 2);
    }

    #[test]
    fn test_value_type_name() {
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(Value::Bool(true).type_name(), "boolean");
        assert_eq!(Value::Number(1.0).type_name(), "number");
        assert_eq!(Value::String("x".to_string()).type_name(), "string");
        assert_eq!(Value::Array(vec![]).type_name(), "array");
        assert_eq!(obj(vec![]).type_name(), "object");
    }

    #[test]
    fn test_value_display() {
        assert_eq!(format!("{}", Value::Null), "null");
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::Number(42.0)), "42");
        assert_eq!(format!("{}", Value::Number(3.14)), "3.14");
        assert_eq!(format!("{}", Value::String("hello".to_string())), "hello");
    }

    #[test]
    fn test_value_json_serialization() {
        let val = obj(vec![
            ("name", Value::String("test".to_string())),
            ("count", Value::Number(42.0)),
        ]);
        let json = val.to_json_string_compact();
        assert_eq!(json, r#"{"name":"test","count":42}"#);
    }

    #[test]
    fn test_value_from_serde_json() {
        let json: serde_json::Value = serde_json::json!({
            "name": "test",
            "values": [1, 2, 3],
            "nested": {"a": true}
        });
        let val = Value::from_serde_json(json);
        assert!(val.is_object());
        if let Value::Object(obj) = &val {
            assert_eq!(obj.get("name"), Some(&Value::String("test".to_string())));
            if let Some(Value::Array(arr)) = obj.get("values") {
                assert_eq!(arr.len(), 3);
            } else {
                panic!("Expected array for 'values'");
            }
        }
    }

    #[test]
    fn test_value_to_serde_json_roundtrip() {
        let original = obj(vec![
            ("a", Value::Number(1.0)),
            ("b", Value::String("hello".to_string())),
            ("c", Value::Array(vec![Value::Bool(true), Value::Null])),
        ]);
        let serde_val = original.to_serde_json();
        let roundtrip = Value::from_serde_json(serde_val);
        assert!(deep_equal(&original, &roundtrip));
    }
}
