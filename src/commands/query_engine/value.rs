use indexmap::IndexMap;
use std::fmt;

#[derive(Clone, Debug)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(IndexMap<String, Value>),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => {
                if a.is_nan() && b.is_nan() {
                    return false;
                }
                a == b
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Object(a), Value::Object(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                let mut keys_a: Vec<&String> = a.keys().collect();
                let mut keys_b: Vec<&String> = b.keys().collect();
                keys_a.sort();
                keys_b.sort();
                if keys_a != keys_b {
                    return false;
                }
                for key in keys_a {
                    if a.get(key) != b.get(key) {
                        return false;
                    }
                }
                true
            }
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Number(n) => {
                if n.is_nan() {
                    write!(f, "null")
                } else if n.is_infinite() {
                    if *n > 0.0 {
                        write!(f, "1.7976931348623157e+308")
                    } else {
                        write!(f, "-1.7976931348623157e+308")
                    }
                } else if *n == (*n as i64) as f64 && n.abs() < 1e18 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            Value::String(s) => write!(f, "{}", s),
            Value::Array(_) | Value::Object(_) => {
                write!(f, "{}", self.to_json_string())
            }
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Number(n as f64)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::Array(v)
    }
}

impl From<IndexMap<String, Value>> for Value {
    fn from(m: IndexMap<String, Value>) -> Self {
        Value::Object(m)
    }
}

impl Value {
    /// jq truthiness: false and null are falsy, everything else truthy
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Null | Value::Bool(false))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Bool(_))
    }

    pub fn is_number(&self) -> bool {
        matches!(self, Value::Number(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&IndexMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    fn format_json_string(s: &str) -> String {
        let mut result = std::string::String::from("\"");
        for ch in s.chars() {
            match ch {
                '"' => result.push_str("\\\""),
                '\\' => result.push_str("\\\\"),
                '\n' => result.push_str("\\n"),
                '\r' => result.push_str("\\r"),
                '\t' => result.push_str("\\t"),
                c if (c as u32) < 0x20 => {
                    result.push_str(&format!("\\u{:04x}", c as u32));
                }
                c => result.push(c),
            }
        }
        result.push('"');
        result
    }

    fn format_number(n: f64) -> String {
        if n.is_nan() {
            return "null".to_string();
        }
        if n.is_infinite() {
            return if n > 0.0 {
                "1.7976931348623157e+308".to_string()
            } else {
                "-1.7976931348623157e+308".to_string()
            };
        }
        if n == (n as i64) as f64 && n.abs() < 1e18 {
            format!("{}", n as i64)
        } else {
            format!("{}", n)
        }
    }

    /// Pretty-print JSON with 2-space indentation
    pub fn to_json_string(&self) -> String {
        self.to_json_indent(0)
    }

    fn to_json_indent(&self, indent: usize) -> String {
        let spaces = "  ".repeat(indent);
        let inner_spaces = "  ".repeat(indent + 1);
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(b) => format!("{}", b),
            Value::Number(n) => Self::format_number(*n),
            Value::String(s) => Self::format_json_string(s),
            Value::Array(arr) => {
                if arr.is_empty() {
                    return "[]".to_string();
                }
                let items: Vec<String> = arr
                    .iter()
                    .map(|v| format!("{}{}", inner_spaces, v.to_json_indent(indent + 1)))
                    .collect();
                format!("[\n{}\n{}]", items.join(",\n"), spaces)
            }
            Value::Object(obj) => {
                if obj.is_empty() {
                    return "{}".to_string();
                }
                let items: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}{}: {}",
                            inner_spaces,
                            Self::format_json_string(k),
                            v.to_json_indent(indent + 1)
                        )
                    })
                    .collect();
                format!("{{\n{}\n{}}}", items.join(",\n"), spaces)
            }
        }
    }

    /// Compact JSON serialization
    pub fn to_json_string_compact(&self) -> String {
        match self {
            Value::Null => "null".to_string(),
            Value::Bool(b) => format!("{}", b),
            Value::Number(n) => Self::format_number(*n),
            Value::String(s) => Self::format_json_string(s),
            Value::Array(arr) => {
                let items: Vec<String> =
                    arr.iter().map(|v| v.to_json_string_compact()).collect();
                format!("[{}]", items.join(","))
            }
            Value::Object(obj) => {
                let items: Vec<String> = obj
                    .iter()
                    .map(|(k, v)| {
                        format!("{}:{}", Self::format_json_string(k), v.to_json_string_compact())
                    })
                    .collect();
                format!("{{{}}}", items.join(","))
            }
        }
    }

    /// Convert from serde_json::Value
    pub fn from_serde_json(v: serde_json::Value) -> Value {
        match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => {
                Value::Number(n.as_f64().unwrap_or(0.0))
            }
            serde_json::Value::String(s) => Value::String(s),
            serde_json::Value::Array(arr) => {
                Value::Array(arr.into_iter().map(Value::from_serde_json).collect())
            }
            serde_json::Value::Object(obj) => {
                let mut map = IndexMap::new();
                for (k, v) in obj {
                    map.insert(k, Value::from_serde_json(v));
                }
                Value::Object(map)
            }
        }
    }

    /// Convert to serde_json::Value
    pub fn to_serde_json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::Number(n) => {
                if n.is_nan() || n.is_infinite() {
                    serde_json::Value::Null
                } else {
                    serde_json::json!(*n)
                }
            }
            Value::String(s) => serde_json::Value::String(s.clone()),
            Value::Array(arr) => {
                serde_json::Value::Array(arr.iter().map(|v| v.to_serde_json()).collect())
            }
            Value::Object(obj) => {
                let map: serde_json::Map<String, serde_json::Value> = obj
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_serde_json()))
                    .collect();
                serde_json::Value::Object(map)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_null() {
        let v = Value::Null;
        assert!(v.is_null());
        assert_eq!(v.type_name(), "null");
        assert!(!v.is_truthy());
    }

    #[test]
    fn test_value_bool() {
        let t = Value::Bool(true);
        let f = Value::Bool(false);
        assert!(t.is_bool());
        assert!(t.is_truthy());
        assert!(!f.is_truthy());
        assert_eq!(t.as_bool(), Some(true));
    }

    #[test]
    fn test_value_number() {
        let n = Value::Number(42.0);
        assert!(n.is_number());
        assert_eq!(n.as_f64(), Some(42.0));
        assert_eq!(n.type_name(), "number");
    }

    #[test]
    fn test_value_string() {
        let s = Value::String("hello".to_string());
        assert!(s.is_string());
        assert_eq!(s.as_str(), Some("hello"));
        assert_eq!(s.type_name(), "string");
    }

    #[test]
    fn test_value_array() {
        let arr = Value::Array(vec![Value::Number(1.0), Value::Number(2.0)]);
        assert!(arr.is_array());
        assert_eq!(arr.as_array().unwrap().len(), 2);
        assert_eq!(arr.type_name(), "array");
    }

    #[test]
    fn test_value_object() {
        let mut map = IndexMap::new();
        map.insert("key".to_string(), Value::String("value".to_string()));
        let obj = Value::Object(map);
        assert!(obj.is_object());
        assert_eq!(obj.type_name(), "object");
    }

    #[test]
    fn test_value_from_traits() {
        let v1: Value = true.into();
        assert!(matches!(v1, Value::Bool(true)));

        let v2: Value = 42i64.into();
        assert!(matches!(v2, Value::Number(_)));

        let v3: Value = "test".into();
        assert!(matches!(v3, Value::String(_)));
    }

    #[test]
    fn test_value_equality() {
        assert_eq!(Value::Null, Value::Null);
        assert_eq!(Value::Bool(true), Value::Bool(true));
        assert_eq!(Value::Number(1.0), Value::Number(1.0));
        assert_ne!(Value::Number(1.0), Value::Number(2.0));
    }

    #[test]
    fn test_to_json_string_compact() {
        let arr = Value::Array(vec![Value::Number(1.0), Value::Number(2.0)]);
        assert_eq!(arr.to_json_string_compact(), "[1,2]");
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Value::Null), "null");
        assert_eq!(format!("{}", Value::Bool(true)), "true");
        assert_eq!(format!("{}", Value::Number(42.0)), "42");
        assert_eq!(format!("{}", Value::String("hi".to_string())), "hi");
    }
}
