//! Deterministic JSON serialization.
//!
//! Studio publishes content-addressed files (`launcher.<sha256>.json`), so the same document
//! must always produce the same bytes: object keys are sorted by their UTF-8 bytes, output is
//! indented with two spaces, uses `\n` line endings and ends with a newline. Sorting is done
//! explicitly so the result does not depend on `serde_json`'s `preserve_order` feature.

use serde::Serialize;
use serde_json::Value;

use crate::error::{Error, Result};

/// Serializes `value` into canonical JSON bytes.
pub fn to_canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value).map_err(|e| Error::json("canonical JSON", e))?;
    let mut out = serde_json::to_vec_pretty(&sort_keys(value))
        .map_err(|e| Error::json("canonical JSON", e))?;
    out.push(b'\n');
    Ok(out)
}

fn sort_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, sort_keys(value)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_keys).collect()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keys_are_sorted_recursively() {
        let value = json!({"b": 1, "a": {"z": [ {"y": 1, "x": 2} ], "c": null}});
        let text = String::from_utf8(to_canonical_json(&value).unwrap()).unwrap();
        assert_eq!(
            text,
            "{\n  \"a\": {\n    \"c\": null,\n    \"z\": [\n      {\n        \"x\": 2,\n        \"y\": 1\n      }\n    ]\n  },\n  \"b\": 1\n}\n"
        );
    }

    #[test]
    fn output_is_stable() {
        #[derive(Serialize)]
        struct Doc {
            zeta: u32,
            alpha: &'static str,
        }
        let a = to_canonical_json(&Doc {
            zeta: 1,
            alpha: "ğüşıöç",
        })
        .unwrap();
        let b = to_canonical_json(&json!({"alpha": "ğüşıöç", "zeta": 1})).unwrap();
        assert_eq!(a, b);
        assert!(a.ends_with(b"\n"));
    }
}
