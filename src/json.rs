//! JSON output byte for byte like Python's `json.dumps`, so scripts written
//! against NIRUFLOW 0.x keep working: `indent=2` layout, `", "`/`": "`
//! separators, Python float `repr`, and `ensure_ascii` escaping.
use serde_json::Value;

/// `json.dumps(v, indent=2, ensure_ascii=...)`.
pub fn pretty(v: &Value, ensure_ascii: bool) -> String {
    let mut out = String::new();
    write(&mut out, v, Some(2), 0, ensure_ascii);
    out
}

/// `json.dumps(v)` (no indent).
pub fn compact(v: &Value) -> String {
    let mut out = String::new();
    write(&mut out, v, None, 0, true);
    out
}

fn newline(out: &mut String, indent: Option<usize>, level: usize) {
    if let Some(n) = indent {
        out.push('\n');
        for _ in 0..n * level {
            out.push(' ');
        }
    }
}

fn write(out: &mut String, v: &Value, indent: Option<usize>, level: usize, ascii: bool) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else {
                out.push_str(&py_float(n.as_f64().unwrap_or(0.0)));
            }
        }
        Value::String(s) => string(out, s, ascii),
        Value::Array(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                    if indent.is_none() {
                        out.push(' ');
                    }
                }
                newline(out, indent, level + 1);
                write(out, x, indent, level + 1, ascii);
            }
            newline(out, indent, level);
            out.push(']');
        }
        Value::Object(m) => {
            if m.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, x)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                    if indent.is_none() {
                        out.push(' ');
                    }
                }
                newline(out, indent, level + 1);
                string(out, k, ascii);
                out.push_str(": ");
                write(out, x, indent, level + 1, ascii);
            }
            newline(out, indent, level);
            out.push('}');
        }
    }
}

fn string(out: &mut String, s: &str, ascii: bool) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c if ascii && (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{:04x}", unit));
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Python's `repr(float)`: the shortest round-trip digits, fixed notation for
/// decimal exponents -4..16, else `1e+16` style.
pub fn py_float(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let sci = format!("{:e}", x.abs());
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mant.chars().filter(|c| c.is_ascii_digit()).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let decpt = exp + 1;
    let n = digits.len() as i32;
    let mut s = String::new();
    if x < 0.0 {
        s.push('-');
    }
    if decpt <= -4 || decpt > 16 {
        s.push_str(&digits[..1]);
        if n > 1 {
            s.push('.');
            s.push_str(&digits[1..]);
        }
        let e = decpt - 1;
        s.push('e');
        s.push(if e < 0 { '-' } else { '+' });
        s.push_str(&format!("{:02}", e.abs()));
    } else if decpt <= 0 {
        s.push_str("0.");
        for _ in 0..-decpt {
            s.push('0');
        }
        s.push_str(digits);
    } else if decpt >= n {
        s.push_str(digits);
        for _ in 0..decpt - n {
            s.push('0');
        }
        s.push_str(".0");
    } else {
        s.push_str(&digits[..decpt as usize]);
        s.push('.');
        s.push_str(&digits[decpt as usize..]);
    }
    s
}

/// A JSON number for a float the way Python would store it.
pub fn num(x: f64) -> Value {
    serde_json::Number::from_f64(x)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn floats_print_like_python_repr() {
        for (x, s) in [
            (1.0, "1.0"),
            (0.1, "0.1"),
            (1700000000.123456, "1700000000.123456"),
            (1e16, "1e+16"),
            (1234567890123456.0, "1234567890123456.0"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1.5e-7, "1.5e-07"),
            (-2.5, "-2.5"),
            (0.0, "0.0"),
            (123.0, "123.0"),
            (1e22, "1e+22"),
        ] {
            assert_eq!(py_float(x), s, "{x}");
        }
    }

    #[test]
    fn layout_matches_python_indent_2() {
        let v = json!({"a": [1, 2.5, {"b": []}], "c": {}, "d": "å\"\n", "e": true});
        assert_eq!(
            pretty(&v, true),
            "{\n  \"a\": [\n    1,\n    2.5,\n    {\n      \"b\": []\n    }\n  ],\n  \"c\": {},\n  \"d\": \"\\u00e5\\\"\\n\",\n  \"e\": true\n}"
        );
        assert_eq!(pretty(&json!("å"), false), "\"å\"");
        assert_eq!(
            compact(&json!({"a": 1, "b": "x"})),
            "{\"a\": 1, \"b\": \"x\"}"
        );
        assert_eq!(pretty(&json!("😀"), true), "\"\\ud83d\\ude00\"");
    }
}
