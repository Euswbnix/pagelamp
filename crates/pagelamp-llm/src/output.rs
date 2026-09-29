//! JSON answer formats: the lowest-common-denominator schema every provider takes (OpenAI strict
//! mode, Anthropic `output_config.format`, Ollama `format`, the json_schema of compatible
//! servers), built from a Rust type so there are no hand-written schema strings.
//!
//! The rules (design §3.2): every field required, `additionalProperties: false`, no numeric or
//! string bounds, no formats, no recursion, objects nested at most 5 deep, `anyOf` (never
//! `oneOf`), `enum` (never `const`), and no `$ref`s (inlined).

use schemars::JsonSchema;
use serde_json::{Map, Value};

use crate::request::OutputSpec;

/// Deepest object nesting a format may have.
pub const MAX_OBJECT_DEPTH: usize = 5;

/// Keywords dropped because some provider rejects or ignores them.
const DROPPED: [&str; 22] = [
    "$schema",
    "$id",
    "title",
    "format",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minProperties",
    "maxProperties",
    "default",
    "examples",
    "readOnly",
    "writeOnly",
    "deprecated",
];

/// Why a Rust type can't be an answer format (a programming error; each feature's format has a
/// test).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SchemaProblem {
    #[error("the type refers to itself ({0})")]
    Recursive(String),
    #[error("objects are nested more than {MAX_OBJECT_DEPTH} deep")]
    TooDeep,
    #[error("unresolvable reference {0}")]
    UnknownRef(String),
}

impl OutputSpec {
    /// A JSON answer shaped like `T`, named `name` (letters, digits, `_`).
    pub fn for_type<T: JsonSchema>(name: &'static str) -> Result<OutputSpec, SchemaProblem> {
        let schema =
            serde_json::to_value(schemars::schema_for!(T)).expect("a generated schema is JSON");
        Ok(OutputSpec::Json {
            name,
            schema: lcd_schema(schema)?,
        })
    }
}

/// `schema` rewritten to the lowest common denominator (see the module docs).
pub fn lcd_schema(mut schema: Value) -> Result<Value, SchemaProblem> {
    let defs = match &mut schema {
        Value::Object(map) => {
            let mut defs = map.remove("$defs").unwrap_or(Value::Null);
            if let Some(Value::Object(old)) = map.remove("definitions") {
                if let Value::Object(defs) = &mut defs {
                    defs.extend(old);
                } else {
                    defs = Value::Object(old);
                }
            }
            defs
        }
        _ => Value::Null,
    };
    let mut stack = Vec::new();
    let normalised = normalise(&schema, &defs, &mut stack)?;
    if object_depth(&normalised) > MAX_OBJECT_DEPTH {
        return Err(SchemaProblem::TooDeep);
    }
    Ok(normalised)
}

fn normalise(value: &Value, defs: &Value, stack: &mut Vec<String>) -> Result<Value, SchemaProblem> {
    let Value::Object(map) = value else {
        return Ok(value.clone());
    };
    if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
        if reference == "#" {
            return Err(SchemaProblem::Recursive("the answer type".to_string()));
        }
        let name = reference
            .strip_prefix("#/$defs/")
            .or_else(|| reference.strip_prefix("#/definitions/"))
            .ok_or_else(|| SchemaProblem::UnknownRef(reference.to_string()))?;
        if stack.iter().any(|seen| seen == name) {
            return Err(SchemaProblem::Recursive(name.to_string()));
        }
        let target = defs
            .get(name)
            .ok_or_else(|| SchemaProblem::UnknownRef(reference.to_string()))?;
        stack.push(name.to_string());
        let mut inlined = normalise(target, defs, stack)?;
        stack.pop();
        // A description next to the `$ref` describes this use of it.
        if let (Some(description), Value::Object(inlined)) = (map.get("description"), &mut inlined)
        {
            inlined.insert("description".into(), description.clone());
        }
        return Ok(inlined);
    }
    let mut out = Map::new();
    for (key, child) in map {
        if DROPPED.contains(&key.as_str()) {
            continue;
        }
        let child = match key.as_str() {
            "properties" => {
                let Value::Object(properties) = child else {
                    continue;
                };
                let mut normalised = Map::new();
                for (name, property) in properties {
                    normalised.insert(name.clone(), normalise(property, defs, stack)?);
                }
                Value::Object(normalised)
            }
            "items" | "additionalProperties" if child.is_object() => normalise(child, defs, stack)?,
            "anyOf" | "oneOf" | "allOf" => {
                let Value::Array(options) = child else {
                    continue;
                };
                Value::Array(
                    options
                        .iter()
                        .map(|option| normalise(option, defs, stack))
                        .collect::<Result<_, _>>()?,
                )
            }
            _ => child.clone(),
        };
        match key.as_str() {
            "oneOf" => {
                out.insert("anyOf".into(), child);
            }
            "const" => {
                out.insert("enum".into(), Value::Array(vec![child]));
            }
            _ => {
                out.insert(key.clone(), child);
            }
        }
    }
    if let Some(Value::Object(properties)) = out.get("properties") {
        let required: Vec<Value> = properties.keys().cloned().map(Value::String).collect();
        out.insert("required".into(), Value::Array(required));
        out.insert("additionalProperties".into(), Value::Bool(false));
    }
    Ok(Value::Object(out))
}

/// How deep objects nest in `schema` (a lone object is 1).
fn object_depth(schema: &Value) -> usize {
    let Value::Object(map) = schema else {
        return 0;
    };
    let own = usize::from(map.contains_key("properties"));
    let children = map
        .iter()
        .flat_map(|(key, child)| match (key.as_str(), child) {
            ("properties", Value::Object(properties)) => properties.values().collect::<Vec<_>>(),
            ("anyOf" | "allOf", Value::Array(options)) => options.iter().collect(),
            ("items", child) => vec![child],
            _ => Vec::new(),
        })
        .map(object_depth)
        .max()
        .unwrap_or(0);
    own + children
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    struct Citation {
        handle: String,
        locator: Option<String>,
    }

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(rename_all = "snake_case")]
    enum Kind {
        Read,
        Review,
    }

    #[allow(dead_code)]
    #[derive(JsonSchema)]
    #[serde(tag = "type", rename_all = "snake_case")]
    enum Step {
        Read { minutes: u32 },
        Note { text: String },
    }

    /// A week summary.
    #[allow(dead_code)]
    #[derive(JsonSchema)]
    struct Answer {
        /// One paragraph.
        summary: String,
        minutes: u32,
        kind: Kind,
        citations: Vec<Citation>,
        best: Option<Citation>,
        step: Step,
    }

    /// Every object in `schema`.
    fn objects(schema: &Value) -> Vec<&Map<String, Value>> {
        let mut found = Vec::new();
        if let Value::Object(map) = schema {
            if map.contains_key("properties") {
                found.push(map);
            }
            for child in map.values() {
                match child {
                    Value::Object(_) => found.extend(objects(child)),
                    Value::Array(items) => items.iter().for_each(|i| found.extend(objects(i))),
                    _ => {}
                }
            }
        }
        found
    }

    #[test]
    fn a_rust_type_becomes_the_lowest_common_denominator() {
        let OutputSpec::Json { name, schema } = OutputSpec::for_type::<Answer>("week").unwrap()
        else {
            panic!()
        };
        assert_eq!(name, "week");
        let text = schema.to_string();
        for banned in [
            "$ref", "$defs", "$schema", "oneOf", "const", "format", "minimum", "title",
        ] {
            assert!(!text.contains(banned), "{banned} in {text}");
        }
        for object in objects(&schema) {
            let keys: Vec<&String> = object["properties"].as_object().unwrap().keys().collect();
            let required: Vec<&str> = object["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r.as_str().unwrap())
                .collect();
            assert_eq!(keys.len(), required.len(), "{object:?}");
            assert_eq!(object["additionalProperties"], false);
        }
        // Optional fields stay nullable (and now required).
        assert_eq!(
            schema["properties"]["best"]["anyOf"][1],
            json!({ "type": "null" })
        );
        assert_eq!(
            schema["properties"]["citations"]["items"]["properties"]["locator"]["type"],
            json!(["string", "null"])
        );
        // Tags become one-value enums; field docs are kept.
        assert_eq!(
            schema["properties"]["step"]["anyOf"][0]["properties"]["type"]["enum"],
            json!(["read"])
        );
        assert_eq!(
            schema["properties"]["summary"]["description"],
            "One paragraph."
        );
        assert_eq!(schema["description"], "A week summary.");
    }

    #[test]
    fn the_study_plan_tasks_are_a_valid_answer_format() {
        let OutputSpec::Json { schema, .. } =
            OutputSpec::for_type::<pagelamp_core::planner::PlanTasks>("study_plan_tasks").unwrap()
        else {
            panic!()
        };
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&json!({ "tasks": [{
            "course_id": "folder:demo/course/DEMO101", "kind": "prepare_deadline",
            "title": "Re-read week 4 slides before A2", "description": null,
            "material_ids": ["m1"], "minutes": 45, "priority": "high",
            "earliest": null, "latest": "2026-10-13"
        }] })));
        assert!(!validator.is_valid(&json!({ "tasks": [{ "kind": "write_the_answers" }] })));
    }

    #[test]
    fn recursion_and_deep_nesting_are_refused() {
        #[allow(dead_code)]
        #[derive(JsonSchema)]
        struct Tree {
            children: Vec<Tree>,
        }
        assert!(matches!(
            OutputSpec::for_type::<Tree>("tree"),
            Err(SchemaProblem::Recursive(_))
        ));

        #[allow(dead_code)]
        #[derive(JsonSchema)]
        struct L6 {
            x: u8,
        }
        #[allow(dead_code)]
        #[derive(JsonSchema)]
        struct L5 {
            x: L6,
        }
        #[allow(dead_code)]
        #[derive(JsonSchema)]
        struct L4 {
            x: L5,
        }
        #[allow(dead_code)]
        #[derive(JsonSchema)]
        struct L3 {
            x: Vec<L4>,
        }
        #[allow(dead_code)]
        #[derive(JsonSchema)]
        struct L2 {
            x: L3,
        }
        #[allow(dead_code)]
        #[derive(JsonSchema)]
        struct L1 {
            x: L2,
        }
        assert_eq!(
            OutputSpec::for_type::<L1>("deep"),
            Err(SchemaProblem::TooDeep)
        );
        assert!(OutputSpec::for_type::<L2>("five").is_ok());
    }

    #[test]
    fn the_schema_validates_what_the_type_describes() {
        let OutputSpec::Json { schema, .. } = OutputSpec::for_type::<Answer>("week").unwrap()
        else {
            panic!()
        };
        let validator = jsonschema::validator_for(&schema).unwrap();
        let good = json!({
            "summary": "Week 3", "minutes": 30, "kind": "read",
            "citations": [{ "handle": "c1", "locator": null }],
            "best": null, "step": { "type": "note", "text": "x" }
        });
        assert!(validator.is_valid(&good));
        let mut extra = good.clone();
        extra["surprise"] = json!(1);
        assert!(!validator.is_valid(&extra));
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove("best");
        assert!(!validator.is_valid(&missing));
        let mut wrong = good;
        wrong["kind"] = json!("solve");
        assert!(!validator.is_valid(&wrong));
    }
}
