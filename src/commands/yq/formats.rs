// src/commands/yq/formats.rs
use crate::commands::query_engine::Value;
use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    Yaml,
    Json,
    Xml,
    Ini,
    Csv,
    Toml,
}

#[derive(Debug, Clone)]
pub struct FormatOptions {
    pub input_format: Format,
    pub output_format: Format,
    pub raw: bool,
    pub compact: bool,
    pub pretty_print: bool,
    pub indent: usize,
    pub xml_attribute_prefix: String,
    pub xml_content_name: String,
    pub csv_delimiter: String,
    pub csv_header: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            input_format: Format::Yaml,
            output_format: Format::Yaml,
            raw: false,
            compact: false,
            pretty_print: false,
            indent: 2,
            xml_attribute_prefix: "+@".to_string(),
            xml_content_name: "+content".to_string(),
            csv_delimiter: String::new(),
            csv_header: true,
        }
    }
}

pub struct FrontMatter {
    pub front_matter: Value,
    pub content: String,
}

pub fn detect_format_from_extension(filename: &str) -> Option<Format> {
    let lower = filename.to_lowercase();
    if lower.ends_with(".yaml") || lower.ends_with(".yml") {
        Some(Format::Yaml)
    } else if lower.ends_with(".json") {
        Some(Format::Json)
    } else if lower.ends_with(".xml") {
        Some(Format::Xml)
    } else if lower.ends_with(".ini") {
        Some(Format::Ini)
    } else if lower.ends_with(".csv") || lower.ends_with(".tsv") {
        Some(Format::Csv)
    } else if lower.ends_with(".toml") {
        Some(Format::Toml)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// YAML parsing helpers
// ---------------------------------------------------------------------------

fn serde_yaml_to_value(v: serde_yaml::Value) -> Value {
    match v {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(b),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(i as f64)
            } else if let Some(f) = n.as_f64() {
                Value::Number(f)
            } else {
                Value::Null
            }
        }
        serde_yaml::Value::String(s) => Value::String(s),
        serde_yaml::Value::Sequence(arr) => {
            Value::Array(arr.into_iter().map(serde_yaml_to_value).collect())
        }
        serde_yaml::Value::Mapping(map) => {
            let mut obj = IndexMap::new();
            for (k, v) in map {
                let key = match k {
                    serde_yaml::Value::String(s) => s,
                    serde_yaml::Value::Number(n) => format!("{}", n),
                    serde_yaml::Value::Bool(b) => format!("{}", b),
                    serde_yaml::Value::Null => "null".to_string(),
                    _ => format!("{:?}", k),
                };
                obj.insert(key, serde_yaml_to_value(v));
            }
            Value::Object(obj)
        }
        serde_yaml::Value::Tagged(tagged) => {
            serde_yaml_to_value(tagged.value)
        }
    }
}

fn parse_yaml(input: &str) -> Result<Value, String> {
    let v: serde_yaml::Value = serde_yaml::from_str(input)
        .map_err(|e| format!("YAML parse error: {}", e))?;
    Ok(serde_yaml_to_value(v))
}

pub fn parse_all_yaml_documents(input: &str) -> Vec<Value> {
    let mut results = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(input) {
        if let Ok(v) = serde_yaml::Value::deserialize(doc) {
            results.push(serde_yaml_to_value(v));
        }
    }
    results
}

// ---------------------------------------------------------------------------
// JSON parsing
// ---------------------------------------------------------------------------

fn parse_json(input: &str) -> Result<Value, String> {
    let v: serde_json::Value = serde_json::from_str(input)
        .map_err(|e| format!("JSON parse error: {}", e))?;
    Ok(Value::from_serde_json(v))
}

// ---------------------------------------------------------------------------
// TOML parsing
// ---------------------------------------------------------------------------

fn toml_to_value(v: toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::String(s),
        toml::Value::Integer(i) => Value::Number(i as f64),
        toml::Value::Float(f) => Value::Number(f),
        toml::Value::Boolean(b) => Value::Bool(b),
        toml::Value::Datetime(d) => Value::String(d.to_string()),
        toml::Value::Array(arr) => {
            Value::Array(arr.into_iter().map(toml_to_value).collect())
        }
        toml::Value::Table(tbl) => {
            let mut obj = IndexMap::new();
            for (k, v) in tbl {
                obj.insert(k, toml_to_value(v));
            }
            Value::Object(obj)
        }
    }
}

fn parse_toml(input: &str) -> Result<Value, String> {
    let v: toml::Value = toml::from_str(input)
        .map_err(|e| format!("TOML parse error: {}", e))?;
    Ok(toml_to_value(v))
}

// ---------------------------------------------------------------------------
// CSV parsing
// ---------------------------------------------------------------------------

fn parse_csv(input: &str, options: &FormatOptions) -> Result<Value, String> {
    let delimiter = if options.csv_delimiter.is_empty() {
        b','
    } else {
        options.csv_delimiter.as_bytes()[0]
    };

    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(options.csv_header)
        .delimiter(delimiter)
        .from_reader(input.as_bytes());

    if options.csv_header {
        let headers: Vec<String> = rdr.headers()
            .map_err(|e| format!("CSV parse error: {}", e))?
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut rows = Vec::new();
        for result in rdr.records() {
            let record = result.map_err(|e| format!("CSV parse error: {}", e))?;
            let mut obj = IndexMap::new();
            for (i, field) in record.iter().enumerate() {
                let key = headers.get(i).cloned()
                    .unwrap_or_else(|| format!("column{}", i));
                obj.insert(key, parse_csv_field(field));
            }
            rows.push(Value::Object(obj));
        }
        Ok(Value::Array(rows))
    } else {
        let mut rows = Vec::new();
        for result in rdr.records() {
            let record = result.map_err(|e| format!("CSV parse error: {}", e))?;
            let row: Vec<Value> = record.iter()
                .map(|f| parse_csv_field(f))
                .collect();
            rows.push(Value::Array(row));
        }
        Ok(Value::Array(rows))
    }
}

fn parse_csv_field(field: &str) -> Value {
    if field.is_empty() {
        return Value::String(String::new());
    }
    if let Ok(n) = field.parse::<i64>() {
        return Value::Number(n as f64);
    }
    if let Ok(n) = field.parse::<f64>() {
        return Value::Number(n);
    }
    if field == "true" {
        return Value::Bool(true);
    }
    if field == "false" {
        return Value::Bool(false);
    }
    Value::String(field.to_string())
}

// ---------------------------------------------------------------------------
// INI parsing
// ---------------------------------------------------------------------------

fn parse_ini(input: &str) -> Result<Value, String> {
    let mut root = IndexMap::new();
    let mut current_section: Option<String> = None;

    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed[1..trimmed.len() - 1].trim().to_string();
            current_section = Some(section.clone());
            if !root.contains_key(&section) {
                root.insert(section, Value::Object(IndexMap::new()));
            }
        } else if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim().to_string();
            let val = trimmed[eq_pos + 1..].trim().to_string();
            let parsed_val = parse_ini_value(&val);
            if let Some(ref section) = current_section {
                if let Some(Value::Object(ref mut obj)) = root.get_mut(section) {
                    obj.insert(key, parsed_val);
                }
            } else {
                root.insert(key, parsed_val);
            }
        }
    }
    Ok(Value::Object(root))
}

fn parse_ini_value(val: &str) -> Value {
    if val.is_empty() {
        return Value::String(String::new());
    }
    // Strip surrounding quotes
    let unquoted = if (val.starts_with('"') && val.ends_with('"'))
        || (val.starts_with('\'') && val.ends_with('\''))
    {
        &val[1..val.len() - 1]
    } else {
        val
    };
    if let Ok(n) = unquoted.parse::<i64>() {
        return Value::Number(n as f64);
    }
    if let Ok(n) = unquoted.parse::<f64>() {
        return Value::Number(n);
    }
    if unquoted == "true" {
        return Value::Bool(true);
    }
    if unquoted == "false" {
        return Value::Bool(false);
    }
    Value::String(unquoted.to_string())
}

// ---------------------------------------------------------------------------
// Simple XML parser
// ---------------------------------------------------------------------------

struct XmlParser<'a> {
    input: &'a str,
    pos: usize,
    attr_prefix: String,
    content_name: String,
}

impl<'a> XmlParser<'a> {
    fn new(input: &'a str, attr_prefix: &str, content_name: &str) -> Self {
        Self {
            input,
            pos: 0,
            attr_prefix: attr_prefix.to_string(),
            content_name: content_name.to_string(),
        }
    }

    fn remaining(&self) -> &str {
        &self.input[self.pos..]
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len()
            && self.input.as_bytes()[self.pos].is_ascii_whitespace()
        {
            self.pos += 1;
        }
    }

    fn skip_declaration(&mut self) {
        // Skip <?xml ... ?> and <!-- ... -->
        loop {
            self.skip_whitespace();
            if self.remaining().starts_with("<?") {
                if let Some(end) = self.remaining().find("?>") {
                    self.pos += end + 2;
                } else {
                    break;
                }
            } else if self.remaining().starts_with("<!--") {
                if let Some(end) = self.remaining().find("-->") {
                    self.pos += end + 3;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn parse_document(&mut self) -> Result<Value, String> {
        self.skip_declaration();
        self.skip_whitespace();
        let element = self.parse_element()?;
        Ok(element)
    }

    fn parse_element(&mut self) -> Result<Value, String> {
        self.skip_whitespace();
        if !self.remaining().starts_with('<') {
            return Err("Expected '<'".to_string());
        }
        self.pos += 1; // skip '<'

        let tag_name = self.parse_name()?;
        let mut obj = IndexMap::new();

        // Parse attributes
        loop {
            self.skip_whitespace();
            if self.remaining().starts_with("/>") {
                self.pos += 2;
                let mut wrapper = IndexMap::new();
                wrapper.insert(tag_name, Value::Object(obj));
                return Ok(Value::Object(wrapper));
            }
            if self.remaining().starts_with('>') {
                self.pos += 1;
                break;
            }
            // Parse attribute
            let attr_name = self.parse_name()?;
            self.skip_whitespace();
            if !self.remaining().starts_with('=') {
                return Err("Expected '=' in attribute".to_string());
            }
            self.pos += 1;
            self.skip_whitespace();
            let attr_val = self.parse_attr_value()?;
            let key = format!("{}{}", self.attr_prefix, attr_name);
            obj.insert(key, Value::String(attr_val));
        }

        // Parse children and text content
        let mut children: IndexMap<String, Vec<Value>> = IndexMap::new();
        let mut text_parts = String::new();

        loop {
            if self.remaining().starts_with("</") {
                self.pos += 2;
                let _close_name = self.parse_name()?;
                self.skip_whitespace();
                if self.remaining().starts_with('>') {
                    self.pos += 1;
                }
                break;
            }
            if self.remaining().starts_with("<!--") {
                if let Some(end) = self.remaining().find("-->") {
                    self.pos += end + 3;
                } else {
                    break;
                }
                continue;
            }
            if self.remaining().starts_with('<') {
                let child = self.parse_element()?;
                if let Value::Object(child_obj) = child {
                    for (k, v) in child_obj {
                        children.entry(k).or_default().push(v);
                    }
                }
            } else {
                // Text content
                let start = self.pos;
                while self.pos < self.input.len()
                    && !self.remaining().starts_with('<')
                {
                    self.pos += 1;
                }
                let text = &self.input[start..self.pos];
                text_parts.push_str(text);
            }
        }

        let trimmed_text = text_parts.trim().to_string();
        let has_children = !children.is_empty();
        let has_attrs = !obj.is_empty();
        let has_text = !trimmed_text.is_empty();

        if !has_children && !has_attrs && has_text {
            // Simple text element
            let mut wrapper = IndexMap::new();
            wrapper.insert(tag_name, Value::String(trimmed_text));
            return Ok(Value::Object(wrapper));
        }

        // Add children
        for (k, vals) in children {
            if vals.len() == 1 {
                obj.insert(k, vals.into_iter().next().unwrap());
            } else {
                obj.insert(k, Value::Array(vals));
            }
        }

        // Add text content if mixed
        if has_text && (has_children || has_attrs) {
            obj.insert(self.content_name.clone(), Value::String(trimmed_text));
        }

        let mut wrapper = IndexMap::new();
        wrapper.insert(tag_name, Value::Object(obj));
        Ok(Value::Object(wrapper))
    }

    fn parse_name(&mut self) -> Result<String, String> {
        let start = self.pos;
        while self.pos < self.input.len() {
            let c = self.input.as_bytes()[self.pos];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
                || c == b'.' || c == b':'
            {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err("Expected name".to_string());
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn parse_attr_value(&mut self) -> Result<String, String> {
        let quote = if self.remaining().starts_with('"') {
            '"'
        } else if self.remaining().starts_with('\'') {
            '\''
        } else {
            return Err("Expected quote".to_string());
        };
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.input.len()
            && self.input.as_bytes()[self.pos] as char != quote
        {
            self.pos += 1;
        }
        let val = self.input[start..self.pos].to_string();
        self.pos += 1; // skip closing quote
        Ok(val)
    }
}

fn parse_xml(input: &str, options: &FormatOptions) -> Result<Value, String> {
    let mut parser = XmlParser::new(
        input,
        &options.xml_attribute_prefix,
        &options.xml_content_name,
    );
    parser.parse_document()
}

// ---------------------------------------------------------------------------
// Front-matter extraction
// ---------------------------------------------------------------------------

pub fn extract_front_matter(input: &str) -> Option<FrontMatter> {
    let trimmed = input.trim_start();
    if trimmed.starts_with("---") {
        // YAML front-matter
        let after = &trimmed[3..];
        let end_marker = if let Some(pos) = after.find("\n---") {
            pos
        } else if let Some(pos) = after.find("\n...") {
            pos
        } else {
            return None;
        };
        let yaml_str = &after[..end_marker];
        let rest_start = end_marker + 4; // skip \n---
        let content = if rest_start < after.len() {
            after[rest_start..].trim_start_matches('\n').to_string()
        } else {
            String::new()
        };
        match parse_yaml(yaml_str) {
            Ok(fm) => Some(FrontMatter {
                front_matter: fm,
                content,
            }),
            Err(_) => None,
        }
    } else if trimmed.starts_with("+++") {
        // TOML front-matter
        let after = &trimmed[3..];
        if let Some(end) = after.find("\n+++") {
            let toml_str = &after[..end];
            let rest_start = end + 4;
            let content = if rest_start < after.len() {
                after[rest_start..].trim_start_matches('\n').to_string()
            } else {
                String::new()
            };
            match parse_toml(toml_str) {
                Ok(fm) => Some(FrontMatter {
                    front_matter: fm,
                    content,
                }),
                Err(_) => None,
            }
        } else {
            None
        }
    } else if trimmed.starts_with("{{{") {
        // JSON front-matter
        let after = &trimmed[3..];
        if let Some(end) = after.find("}}}") {
            let json_str = &after[..end];
            let rest_start = end + 3;
            let content = if rest_start < after.len() {
                after[rest_start..].trim_start_matches('\n').to_string()
            } else {
                String::new()
            };
            match parse_json(json_str) {
                Ok(fm) => Some(FrontMatter {
                    front_matter: fm,
                    content,
                }),
                Err(_) => None,
            }
        } else {
            None
        }
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Main parse_input
// ---------------------------------------------------------------------------

pub fn parse_input(input: &str, options: &FormatOptions) -> Result<Value, String> {
    match options.input_format {
        Format::Yaml => parse_yaml(input),
        Format::Json => parse_json(input),
        Format::Toml => parse_toml(input),
        Format::Csv => parse_csv(input, options),
        Format::Ini => parse_ini(input),
        Format::Xml => parse_xml(input, options),
    }
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

pub fn format_output(value: &Value, options: &FormatOptions) -> String {
    match options.output_format {
        Format::Yaml => format_yaml(value),
        Format::Json => format_json_output(value, options),
        Format::Toml => format_toml_output(value),
        Format::Csv => format_csv_output(value, options),
        Format::Ini => format_ini_output(value),
        Format::Xml => format_xml_output(value, options),
    }
}

fn value_to_serde_yaml(v: &Value) -> serde_yaml::Value {
    match v {
        Value::Null => serde_yaml::Value::Null,
        Value::Bool(b) => serde_yaml::Value::Bool(*b),
        Value::Number(n) => {
            if *n == (*n as i64) as f64 && n.abs() < 1e18 {
                serde_yaml::Value::Number(serde_yaml::Number::from(*n as i64))
            } else {
                serde_yaml::Value::Number(serde_yaml::Number::from(*n))
            }
        }
        Value::String(s) => serde_yaml::Value::String(s.clone()),
        Value::Array(arr) => {
            serde_yaml::Value::Sequence(
                arr.iter().map(value_to_serde_yaml).collect(),
            )
        }
        Value::Object(obj) => {
            let mut map = serde_yaml::Mapping::new();
            for (k, v) in obj {
                map.insert(
                    serde_yaml::Value::String(k.clone()),
                    value_to_serde_yaml(v),
                );
            }
            serde_yaml::Value::Mapping(map)
        }
    }
}

fn format_yaml(value: &Value) -> String {
    let yaml_val = value_to_serde_yaml(value);
    let mut s = serde_yaml::to_string(&yaml_val).unwrap_or_default();
    // serde_yaml adds trailing newline, remove it for consistency
    if s.ends_with('\n') {
        s.pop();
    }
    s
}

fn format_json_output(value: &Value, options: &FormatOptions) -> String {
    if options.compact {
        value.to_json_string_compact()
    } else {
        value.to_json_string()
    }
}

fn value_to_toml(v: &Value) -> toml::Value {
    match v {
        Value::Null => toml::Value::String("null".to_string()),
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Number(n) => {
            if *n == (*n as i64) as f64 && n.abs() < 1e18 {
                toml::Value::Integer(*n as i64)
            } else {
                toml::Value::Float(*n)
            }
        }
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Array(arr) => {
            toml::Value::Array(arr.iter().map(value_to_toml).collect())
        }
        Value::Object(obj) => {
            let mut tbl = toml::map::Map::new();
            for (k, v) in obj {
                tbl.insert(k.clone(), value_to_toml(v));
            }
            toml::Value::Table(tbl)
        }
    }
}

fn format_toml_output(value: &Value) -> String {
    let toml_val = value_to_toml(value);
    match toml_val {
        toml::Value::Table(tbl) => {
            toml::to_string_pretty(&tbl).unwrap_or_default()
                .trim_end().to_string()
        }
        _ => toml::to_string_pretty(&toml_val).unwrap_or_default()
                .trim_end().to_string(),
    }
}

fn format_csv_output(value: &Value, options: &FormatOptions) -> String {
    let delimiter = if options.csv_delimiter.is_empty() {
        b','
    } else {
        options.csv_delimiter.as_bytes()[0]
    };

    let arr = match value {
        Value::Array(a) => a,
        _ => return String::new(),
    };

    if arr.is_empty() {
        return String::new();
    }

    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());

    if let Some(Value::Object(first)) = arr.first() {
        if options.csv_header {
            let headers: Vec<String> = first.keys().cloned().collect();
            let _ = wtr.write_record(&headers);
        }
        for item in arr {
            if let Value::Object(obj) = item {
                let fields: Vec<String> = if let Some(Value::Object(f)) = arr.first() {
                    f.keys().map(|k| {
                        value_to_csv_field(obj.get(k).unwrap_or(&Value::Null))
                    }).collect()
                } else {
                    obj.values().map(value_to_csv_field).collect()
                };
                let _ = wtr.write_record(&fields);
            }
        }
    } else {
        for item in arr {
            if let Value::Array(row) = item {
                let fields: Vec<String> = row.iter()
                    .map(value_to_csv_field).collect();
                let _ = wtr.write_record(&fields);
            }
        }
    }

    let data = wtr.into_inner().unwrap_or_default();
    let s = String::from_utf8(data).unwrap_or_default();
    s.trim_end().to_string()
}

fn value_to_csv_field(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => format!("{}", b),
        Value::Number(n) => {
            if *n == (*n as i64) as f64 && n.abs() < 1e18 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::String(s) => s.clone(),
        _ => v.to_json_string_compact(),
    }
}

fn format_ini_output(value: &Value) -> String {
    let obj = match value {
        Value::Object(o) => o,
        _ => return String::new(),
    };

    let mut lines = Vec::new();
    for (k, v) in obj {
        if !matches!(v, Value::Object(_)) {
            lines.push(format!("{} = {}", k, ini_value_str(v)));
        }
    }
    for (k, v) in obj {
        if let Value::Object(section) = v {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("[{}]", k));
            for (sk, sv) in section {
                lines.push(format!("{} = {}", sk, ini_value_str(sv)));
            }
        }
    }
    lines.join("\n")
}

fn ini_value_str(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => format!("{}", b),
        Value::Number(n) => {
            if *n == (*n as i64) as f64 && n.abs() < 1e18 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::String(s) => s.clone(),
        _ => v.to_json_string_compact(),
    }
}

fn format_xml_output(value: &Value, options: &FormatOptions) -> String {
    let mut output = String::new();
    write_xml_value(&mut output, value, options, 0);
    output.trim_end().to_string()
}

fn write_xml_value(
    out: &mut String,
    value: &Value,
    options: &FormatOptions,
    depth: usize,
) {
    let indent = " ".repeat(options.indent * depth);
    match value {
        Value::Object(obj) => {
            for (tag, val) in obj {
                match val {
                    Value::Object(inner) => {
                        let mut attrs = String::new();
                        let mut children = Vec::new();
                        let mut text_content = None;

                        for (k, v) in inner {
                            if k.starts_with(&options.xml_attribute_prefix) {
                                let attr_name =
                                    &k[options.xml_attribute_prefix.len()..];
                                if let Value::String(s) = v {
                                    attrs.push_str(&format!(
                                        " {}=\"{}\"", attr_name, s
                                    ));
                                }
                            } else if k == &options.xml_content_name {
                                if let Value::String(s) = v {
                                    text_content = Some(s.clone());
                                }
                            } else {
                                children.push((k.clone(), v.clone()));
                            }
                        }

                        if children.is_empty() {
                            if let Some(text) = text_content {
                                out.push_str(&format!(
                                    "{}<{}{}>{}</{}>\n",
                                    indent, tag, attrs, text, tag
                                ));
                            } else {
                                out.push_str(&format!(
                                    "{}<{}{}/>\n", indent, tag, attrs
                                ));
                            }
                        } else {
                            out.push_str(&format!(
                                "{}<{}{}>\n", indent, tag, attrs
                            ));
                            for (k, v) in &children {
                                let mut child_obj = IndexMap::new();
                                child_obj.insert(k.clone(), v.clone());
                                write_xml_value(
                                    out,
                                    &Value::Object(child_obj),
                                    options,
                                    depth + 1,
                                );
                            }
                            if let Some(text) = text_content {
                                let ci = " ".repeat(
                                    options.indent * (depth + 1),
                                );
                                out.push_str(&format!("{}{}\n", ci, text));
                            }
                            out.push_str(&format!("{}</{}>\n", indent, tag));
                        }
                    }
                    Value::String(s) => {
                        out.push_str(&format!(
                            "{}<{}>{}</{}>\n", indent, tag, s, tag
                        ));
                    }
                    Value::Array(arr) => {
                        for item in arr {
                            let mut wrapper = IndexMap::new();
                            wrapper.insert(tag.clone(), item.clone());
                            write_xml_value(
                                out,
                                &Value::Object(wrapper),
                                options,
                                depth,
                            );
                        }
                    }
                    _ => {
                        out.push_str(&format!(
                            "{}<{}>{}</{}>\n", indent, tag, val, tag
                        ));
                    }
                }
            }
        }
        _ => {
            out.push_str(&format!("{}{}\n", indent, value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_opts() -> FormatOptions {
        FormatOptions::default()
    }

    fn opts_with_input(fmt: Format) -> FormatOptions {
        let mut o = FormatOptions::default();
        o.input_format = fmt;
        o
    }

    fn opts_with_output(fmt: Format) -> FormatOptions {
        let mut o = FormatOptions::default();
        o.output_format = fmt;
        o
    }

    #[test]
    fn test_parse_yaml_object() {
        let input = "name: hello\nage: 30";
        let result = parse_input(input, &opts_with_input(Format::Yaml)).unwrap();
        if let Value::Object(obj) = &result {
            assert_eq!(obj.get("name"), Some(&Value::String("hello".to_string())));
            assert_eq!(obj.get("age"), Some(&Value::Number(30.0)));
        } else {
            panic!("Expected object, got {:?}", result);
        }
    }

    #[test]
    fn test_parse_json_object() {
        let input = r#"{"a":1,"b":"hello"}"#;
        let result = parse_input(input, &opts_with_input(Format::Json)).unwrap();
        if let Value::Object(obj) = &result {
            assert_eq!(obj.get("a"), Some(&Value::Number(1.0)));
            assert_eq!(obj.get("b"), Some(&Value::String("hello".to_string())));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_parse_toml_nested() {
        let input = "[package]\nname = \"test\"\nversion = \"1.0\"";
        let result = parse_input(input, &opts_with_input(Format::Toml)).unwrap();
        if let Value::Object(obj) = &result {
            if let Some(Value::Object(pkg)) = obj.get("package") {
                assert_eq!(
                    pkg.get("name"),
                    Some(&Value::String("test".to_string()))
                );
            } else {
                panic!("Expected package object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_parse_csv_with_header() {
        let input = "name,age\nAlice,30\nBob,25";
        let result = parse_input(input, &opts_with_input(Format::Csv)).unwrap();
        if let Value::Array(arr) = &result {
            assert_eq!(arr.len(), 2);
            if let Value::Object(row) = &arr[0] {
                assert_eq!(
                    row.get("name"),
                    Some(&Value::String("Alice".to_string()))
                );
                assert_eq!(row.get("age"), Some(&Value::Number(30.0)));
            }
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_parse_csv_without_header() {
        let mut opts = opts_with_input(Format::Csv);
        opts.csv_header = false;
        let input = "Alice,30\nBob,25";
        let result = parse_input(input, &opts).unwrap();
        if let Value::Array(arr) = &result {
            assert_eq!(arr.len(), 2);
            if let Value::Array(row) = &arr[0] {
                assert_eq!(row[0], Value::String("Alice".to_string()));
                assert_eq!(row[1], Value::Number(30.0));
            }
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_parse_ini() {
        let input = "[database]\nhost = localhost\nport = 5432";
        let result = parse_input(input, &opts_with_input(Format::Ini)).unwrap();
        if let Value::Object(obj) = &result {
            if let Some(Value::Object(db)) = obj.get("database") {
                assert_eq!(
                    db.get("host"),
                    Some(&Value::String("localhost".to_string()))
                );
                assert_eq!(db.get("port"), Some(&Value::Number(5432.0)));
            } else {
                panic!("Expected database section");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_parse_xml_simple() {
        let input = "<root><name>hello</name></root>";
        let result = parse_input(input, &opts_with_input(Format::Xml)).unwrap();
        if let Value::Object(obj) = &result {
            if let Some(Value::Object(root)) = obj.get("root") {
                assert_eq!(
                    root.get("name"),
                    Some(&Value::String("hello".to_string()))
                );
            } else {
                panic!("Expected root object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_format_yaml_output() {
        let mut obj = IndexMap::new();
        obj.insert("name".to_string(), Value::String("hello".to_string()));
        obj.insert("age".to_string(), Value::Number(30.0));
        let value = Value::Object(obj);
        let output = format_output(&value, &opts_with_output(Format::Yaml));
        assert!(output.contains("name: hello"));
        assert!(output.contains("age: 30"));
    }

    #[test]
    fn test_format_json_output_pretty() {
        let mut obj = IndexMap::new();
        obj.insert("a".to_string(), Value::Number(1.0));
        let value = Value::Object(obj);
        let output = format_output(&value, &opts_with_output(Format::Json));
        assert!(output.contains("\"a\": 1"));
    }

    #[test]
    fn test_format_json_output_compact() {
        let mut opts = opts_with_output(Format::Json);
        opts.compact = true;
        let mut obj = IndexMap::new();
        obj.insert("a".to_string(), Value::Number(1.0));
        let value = Value::Object(obj);
        let output = format_output(&value, &opts);
        assert_eq!(output, "{\"a\":1}");
    }

    #[test]
    fn test_format_toml_output() {
        let mut obj = IndexMap::new();
        obj.insert("name".to_string(), Value::String("test".to_string()));
        let value = Value::Object(obj);
        let output = format_output(&value, &opts_with_output(Format::Toml));
        assert!(output.contains("name = \"test\""));
    }

    #[test]
    fn test_format_csv_output() {
        let mut row1 = IndexMap::new();
        row1.insert("name".to_string(), Value::String("Alice".to_string()));
        row1.insert("age".to_string(), Value::Number(30.0));
        let value = Value::Array(vec![Value::Object(row1)]);
        let output = format_output(&value, &opts_with_output(Format::Csv));
        assert!(output.contains("name,age"));
        assert!(output.contains("Alice,30"));
    }

    #[test]
    fn test_detect_format_from_extension() {
        assert_eq!(detect_format_from_extension("file.yaml"), Some(Format::Yaml));
        assert_eq!(detect_format_from_extension("file.yml"), Some(Format::Yaml));
        assert_eq!(detect_format_from_extension("file.json"), Some(Format::Json));
        assert_eq!(detect_format_from_extension("file.xml"), Some(Format::Xml));
        assert_eq!(detect_format_from_extension("file.ini"), Some(Format::Ini));
        assert_eq!(detect_format_from_extension("file.csv"), Some(Format::Csv));
        assert_eq!(detect_format_from_extension("file.tsv"), Some(Format::Csv));
        assert_eq!(detect_format_from_extension("file.toml"), Some(Format::Toml));
        assert_eq!(detect_format_from_extension("file.txt"), None);
    }

    #[test]
    fn test_extract_yaml_front_matter() {
        let input = "---\ntitle: Hello\n---\nBody content here";
        let fm = extract_front_matter(input).unwrap();
        if let Value::Object(obj) = &fm.front_matter {
            assert_eq!(
                obj.get("title"),
                Some(&Value::String("Hello".to_string()))
            );
        } else {
            panic!("Expected object");
        }
        assert!(fm.content.contains("Body content here"));
    }

    #[test]
    fn test_multi_document_yaml() {
        let input = "name: first\n---\nname: second";
        let docs = parse_all_yaml_documents(input);
        assert_eq!(docs.len(), 2);
        if let Value::Object(obj) = &docs[0] {
            assert_eq!(
                obj.get("name"),
                Some(&Value::String("first".to_string()))
            );
        }
        if let Value::Object(obj) = &docs[1] {
            assert_eq!(
                obj.get("name"),
                Some(&Value::String("second".to_string()))
            );
        }
    }

    #[test]
    fn test_parse_xml_with_attributes() {
        let input = r#"<book id="1"><title>Rust</title></book>"#;
        let result = parse_input(input, &opts_with_input(Format::Xml)).unwrap();
        if let Value::Object(obj) = &result {
            if let Some(Value::Object(book)) = obj.get("book") {
                assert_eq!(
                    book.get("+@id"),
                    Some(&Value::String("1".to_string()))
                );
                assert_eq!(
                    book.get("title"),
                    Some(&Value::String("Rust".to_string()))
                );
            } else {
                panic!("Expected book object");
            }
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_format_ini_output() {
        let mut db = IndexMap::new();
        db.insert("host".to_string(), Value::String("localhost".to_string()));
        db.insert("port".to_string(), Value::Number(5432.0));
        let mut obj = IndexMap::new();
        obj.insert("database".to_string(), Value::Object(db));
        let value = Value::Object(obj);
        let output = format_output(&value, &opts_with_output(Format::Ini));
        assert!(output.contains("[database]"));
        assert!(output.contains("host = localhost"));
        assert!(output.contains("port = 5432"));
    }
}