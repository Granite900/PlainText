//! HTTP + JSON helpers for `import web`.
//!
//! Local paths (anything not starting with `http://` or `https://`) are read
//! from disk so examples and CI can run offline. Real URLs use `ureq` with a
//! short timeout and readable error messages.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::diagnostics::Diagnostic;
use crate::token::Span;
use crate::value::{env_declare, Env, MapKey, PtMap, Value};

const TIMEOUT: Duration = Duration::from_secs(15);

/// Convert a PlainText value into JSON text.
pub fn to_json(v: &Value) -> Result<String, String> {
    let j = value_to_json(v)?;
    serde_json::to_string(&j).map_err(|e| e.to_string())
}

/// Parse JSON text into a PlainText value.
pub fn parse_json(text: &str, span: Span) -> Result<Value, Diagnostic> {
    let j: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        Diagnostic::new(span, format!("couldn't parse that JSON: {}", e))
            .with_hint("JSON must be an object, array, number, text, true/false, or null")
    })?;
    Ok(json_to_value(&j))
}

/// GET a URL or local file and return the body as Text.
pub fn http_get(url: &str, span: Span) -> Result<String, Diagnostic> {
    if is_remote(url) {
        let resp = ureq::get(url)
            .timeout(TIMEOUT)
            .call()
            .map_err(|e| http_err(span, "get", url, &e))?;
        resp.into_string()
            .map_err(|e| Diagnostic::new(span, format!("couldn't read the response from \"{}\": {}", url, e)))
    } else {
        std::fs::read_to_string(url).map_err(|e| {
            Diagnostic::new(span, format!("couldn't read file \"{}\": {}", url, e))
                .with_hint("for the web, use an https:// address; for offline demos, use a local path")
        })
    }
}

/// GET URL/file and parse the body as JSON.
pub fn http_get_json(url: &str, span: Span) -> Result<Value, Diagnostic> {
    let body = http_get(url, span)?;
    parse_json(&body, span)
}

/// POST JSON to a remote URL; returns the response body as Text.
pub fn http_post_json(url: &str, payload: &Value, span: Span) -> Result<String, Diagnostic> {
    if !is_remote(url) {
        return Err(Diagnostic::new(
            span,
            "post_json only works with http:// or https:// addresses",
        ));
    }
    let body = to_json(payload)
        .map_err(|e| Diagnostic::new(span, format!("couldn't turn that value into JSON: {}", e)))?;
    let resp = ureq::post(url)
        .set("Content-Type", "application/json")
        .timeout(TIMEOUT)
        .send_string(&body)
        .map_err(|e| http_err(span, "post", url, &e))?;
    resp.into_string()
        .map_err(|e| Diagnostic::new(span, format!("couldn't read the response from \"{}\": {}", url, e)))
}

fn is_remote(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    u.starts_with("http://") || u.starts_with("https://")
}

fn http_err(span: Span, verb: &str, url: &str, err: &ureq::Error) -> Diagnostic {
    Diagnostic::new(span, format!("couldn't {} \"{}\": {}", verb, url, err))
        .with_hint("check the address, your network connection, and that the site allows requests")
}

fn value_to_json(v: &Value) -> Result<serde_json::Value, String> {
    Ok(match v {
        Value::Number(n) => {
            if let Some(i) = num_as_i64(*n) {
                serde_json::Value::Number(i.into())
            } else if let Some(num) = serde_json::Number::from_f64(*n) {
                serde_json::Value::Number(num)
            } else {
                return Err("that number can't be written as JSON".into());
            }
        }
        Value::Text(s) => serde_json::Value::String((**s).clone()),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Nothing => serde_json::Value::Null,
        Value::List(items) => {
            let mut arr = Vec::new();
            for item in items.borrow().iter() {
                arr.push(value_to_json(item)?);
            }
            serde_json::Value::Array(arr)
        }
        Value::Dictionary(map) => {
            let mut obj = serde_json::Map::new();
            for (k, val) in map.borrow().entries.iter() {
                let key = match k {
                    MapKey::Text(s) => s.clone(),
                    MapKey::Number(n) => crate::value::format_number(*n),
                    MapKey::Bool(b) => {
                        if *b {
                            "true".into()
                        } else {
                            "false".into()
                        }
                    }
                };
                obj.insert(key, value_to_json(val)?);
            }
            serde_json::Value::Object(obj)
        }
        other => {
            return Err(format!(
                "can't turn a {} into JSON (use numbers, text, booleans, lists, dictionaries, or nothing)",
                other.type_name()
            ));
        }
    })
}

fn json_to_value(j: &serde_json::Value) -> Value {
    match j {
        serde_json::Value::Null => Value::Nothing,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Value::text(s.clone()),
        serde_json::Value::Array(items) => {
            let list: Vec<Value> = items.iter().map(json_to_value).collect();
            Value::List(Rc::new(RefCell::new(list)))
        }
        serde_json::Value::Object(map) => {
            let mut pt = PtMap::new();
            for (k, v) in map.iter() {
                pt.set(MapKey::Text(k.clone()), json_to_value(v));
            }
            Value::Dictionary(Rc::new(RefCell::new(pt)))
        }
    }
}

fn num_as_i64(n: f64) -> Option<i64> {
    if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
        Some(n as i64)
    } else {
        None
    }
}

/// After `import web`, bind the `web` helper object into globals.
pub fn install(globals: &Env) {
    env_declare(globals, "web", Value::WebModule);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;
    use crate::value::MapKey;

    #[test]
    fn json_round_trip_basics() {
        let mut map = PtMap::new();
        map.set(MapKey::Text("n".into()), Value::Number(3.0));
        map.set(MapKey::Text("ok".into()), Value::Bool(true));
        map.set(MapKey::Text("t".into()), Value::text("hi"));
        let v = Value::Dictionary(Rc::new(RefCell::new(map)));
        let text = to_json(&v).unwrap();
        let back = parse_json(&text, Span::new(0, 0)).unwrap();
        match back {
            Value::Dictionary(m) => {
                let m = m.borrow();
                match m.get(&MapKey::Text("n".into())) {
                    Some(Value::Number(n)) => assert!((n - 3.0).abs() < 1e-9),
                    other => panic!("bad n: {:?}", other.map(|v| v.display())),
                }
                match m.get(&MapKey::Text("ok".into())) {
                    Some(Value::Bool(true)) => {}
                    other => panic!("bad ok: {:?}", other.map(|v| v.display())),
                }
            }
            other => panic!("expected dictionary, got {}", other.type_name()),
        }
    }

    #[test]
    fn local_get_json_reads_fixture_shape() {
        let span = Span::new(0, 0);
        // Relative to repo root when tests run via cargo.
        let v = http_get_json("examples/fixtures/sample.json", span).expect("fixture");
        match v {
            Value::Dictionary(m) => match m.borrow().get(&MapKey::Text("name".into())) {
                Some(Value::Text(s)) => assert_eq!(&**s, "Ada"),
                other => panic!("bad name: {:?}", other.map(|v| v.display())),
            },
            other => panic!("expected dictionary, got {}", other.type_name()),
        }
    }
}
