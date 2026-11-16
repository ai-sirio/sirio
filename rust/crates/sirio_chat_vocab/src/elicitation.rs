//! A form an MCP server asked the user to fill, in terms no transport owns.
//!
//! MCP's elicitation schema is deliberately flat: primitives, enums, nothing
//! nested. [`elicitation_fields`] reads that subset into a list a surface can
//! draw, and refuses anything outside it with a reason, so the caller can
//! decline the request instead of drawing a form it cannot honour.

use serde_json::{Map, Value};

/// One input of a form.
#[derive(Clone, Debug, PartialEq)]
pub struct ElicitationField {
    /// The property name; the key of the answer.
    pub name: String,
    /// What the user reads: the schema's `title`, else the name.
    pub title: String,
    pub description: Option<String>,
    pub required: bool,
    pub kind: ElicitationFieldKind,
    /// The schema's `default`, verbatim.
    pub default: Option<Value>,
}

/// What a field accepts.
#[derive(Clone, Debug, PartialEq)]
pub enum ElicitationFieldKind {
    /// Free text; `format` is the schema's hint (`email`, `uri`, ...).
    Text {
        format: Option<String>,
    },
    Number,
    Integer,
    Boolean,
    /// One of a closed set.
    Choice(Vec<ElicitationChoice>),
}

/// One option of a [`ElicitationFieldKind::Choice`].
#[derive(Clone, Debug, PartialEq)]
pub struct ElicitationChoice {
    /// What goes on the wire.
    pub value: String,
    /// What the user reads.
    pub label: String,
}

/// What the server asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum ElicitationPrompt {
    Form(Vec<ElicitationField>),
    /// Send the user to a page; the answer carries no data.
    Url {
        url: String,
        elicitation_id: Option<String>,
    },
    /// A request this build cannot draw; the reason is shown and the
    /// request declined.
    Unsupported {
        reason: String,
    },
}

/// How the user answered.
#[derive(Clone, Debug, PartialEq)]
pub enum ElicitationAction {
    Accept(Map<String, Value>),
    Decline,
    Cancel,
}

/// MCP's restricted elicitation schema. Order: `order` when given (names it lists that the schema
/// lacks are skipped; properties it omits follow in schema order), else schema order.
///
/// # Errors
/// A reason when the schema is outside the flat subset: not an object, a
/// nested or array property, an untyped property, or a `required` name that
/// is not a property.
pub fn elicitation_fields(
    schema: &Value,
    order: Option<&[String]>,
) -> Result<Vec<ElicitationField>, String> {
    let root = schema
        .as_object()
        .ok_or_else(|| "the form schema is not an object".to_string())?;
    match root.get("type").and_then(Value::as_str) {
        None | Some("object") => {}
        Some(other) => {
            return Err(format!(
                "the form schema's root is a {other}, not an object"
            ));
        }
    }
    let empty = Map::new();
    let properties = match root.get("properties") {
        None => &empty,
        Some(value) => value
            .as_object()
            .ok_or_else(|| "the form schema's properties are not an object".to_string())?,
    };
    let required: Vec<&str> = match root.get("required") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .and_then(|names| names.iter().map(Value::as_str).collect())
            .ok_or_else(|| "the form schema's `required` is not a list of names".to_string())?,
    };
    if let Some(missing) = required
        .iter()
        .find(|name| !properties.contains_key(**name))
    {
        return Err(format!("`{missing}` is required but is not a property"));
    }

    let mut sequence: Vec<&str> = Vec::with_capacity(properties.len());
    for name in order.unwrap_or_default() {
        if properties.contains_key(name) && !sequence.contains(&name.as_str()) {
            sequence.push(name);
        }
    }
    for name in properties.keys() {
        if !sequence.contains(&name.as_str()) {
            sequence.push(name);
        }
    }

    sequence
        .into_iter()
        .map(|name| field_of(name, &properties[name], required.contains(&name)))
        .collect()
}

fn field_of(name: &str, property: &Value, required: bool) -> Result<ElicitationField, String> {
    let object = property
        .as_object()
        .ok_or_else(|| format!("`{name}` is not a schema object"))?;
    let text = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let kind = match object.get("type").and_then(Value::as_str) {
        None => return Err(format!("`{name}` has no type")),
        Some("string") => match choices(name, object)? {
            Some(options) => ElicitationFieldKind::Choice(options),
            None => ElicitationFieldKind::Text {
                format: text("format"),
            },
        },
        Some("number") => ElicitationFieldKind::Number,
        Some("integer") => ElicitationFieldKind::Integer,
        Some("boolean") => ElicitationFieldKind::Boolean,
        Some(other) => {
            return Err(format!(
                "`{name}` is a {other}, which a form cannot ask for"
            ));
        }
    };
    Ok(ElicitationField {
        name: name.to_string(),
        title: text("title").unwrap_or_else(|| name.to_string()),
        description: text("description"),
        required,
        kind,
        default: object.get("default").cloned(),
    })
}

/// The options of a string property, from `oneOf` (`const` + `title`) or
/// `enum` (+ `enumNames`). `None` when it has neither.
fn choices(
    name: &str,
    object: &Map<String, Value>,
) -> Result<Option<Vec<ElicitationChoice>>, String> {
    if let Some(one_of) = object.get("oneOf") {
        let one_of = one_of
            .as_array()
            .filter(|options| !options.is_empty())
            .ok_or_else(|| format!("`{name}` has no options to choose from"))?;
        return one_of
            .iter()
            .map(|option| {
                let value = option
                    .get("const")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("an option of `{name}` has no string const"))?;
                let label = option.get("title").and_then(Value::as_str).unwrap_or(value);
                Ok(ElicitationChoice {
                    value: value.to_string(),
                    label: label.to_string(),
                })
            })
            .collect::<Result<_, String>>()
            .map(Some);
    }
    let Some(values) = object.get("enum") else {
        return Ok(None);
    };
    let values = values
        .as_array()
        .filter(|options| !options.is_empty())
        .ok_or_else(|| format!("`{name}` has no options to choose from"))?;
    let labels = object.get("enumNames").and_then(Value::as_array);
    if labels.is_some_and(|labels| labels.len() != values.len()) {
        return Err(format!(
            "`{name}` names a different number of options than it lists"
        ));
    }
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let value = value
                .as_str()
                .ok_or_else(|| format!("an option of `{name}` is not a string"))?;
            let label = labels
                .and_then(|labels| labels.get(index))
                .and_then(Value::as_str)
                .unwrap_or(value);
            Ok(ElicitationChoice {
                value: value.to_string(),
                label: label.to_string(),
            })
        })
        .collect::<Result<_, String>>()
        .map(Some)
}

/// The JSON value a typed or picked answer stands for. `Ok(None)` for an empty optional answer.
///
/// # Errors
/// A sentence for the user when the answer is empty but required, or is not
/// a value of the field's kind.
pub fn elicitation_value(field: &ElicitationField, typed: &str) -> Result<Option<Value>, String> {
    let typed = typed.trim();
    if typed.is_empty() {
        return if field.required {
            Err(format!("{} is required", field.title))
        } else {
            Ok(None)
        };
    }
    let bad = |expected: &str| format!("{} must be {expected}", field.title);
    let value = match &field.kind {
        ElicitationFieldKind::Text { .. } => Value::String(typed.to_string()),
        ElicitationFieldKind::Integer => typed
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| bad("a whole number"))?,
        ElicitationFieldKind::Number => typed
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .ok_or_else(|| bad("a number"))?,
        ElicitationFieldKind::Boolean => match typed.to_ascii_lowercase().as_str() {
            "true" | "yes" | "1" => Value::Bool(true),
            "false" | "no" | "0" => Value::Bool(false),
            _ => return Err(bad("yes or no")),
        },
        // A value is exact and wins; a label only stands in when it is
        // unambiguous, so an earlier option's label can never shadow a
        // later option's value.
        ElicitationFieldKind::Choice(options) => {
            let by_label: Vec<_> = options.iter().filter(|o| o.label == typed).collect();
            options
                .iter()
                .find(|option| option.value == typed)
                .or_else(|| (by_label.len() == 1).then(|| by_label[0]))
                .map(|option| Value::String(option.value.clone()))
                .ok_or_else(|| bad("one of the options"))?
        }
    };
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    // Ways this unit can fail:
    // - fields come back in alphabetical order (serde_json without
    //   `preserve_order`) instead of the order the server wrote;
    // - `property_order` is ignored, or an unknown name in it panics or
    //   yields a phantom field;
    // - a schema outside the flat subset (nested object, array, untyped
    //   property, dangling `required`) is drawn as a broken form instead of
    //   refused with a reason;
    // - a typed answer reaches the server in the wrong JSON type (a number
    //   as a string), or an invalid one (NaN, 2.5 for an integer) is sent;
    // - an empty answer to a required field is accepted, or to an optional
    //   one is sent as an empty string rather than omitted;
    // - a choice accepts text that is not one of its options, an earlier
    //   option's label shadows a later option's value, or an ambiguous
    //   label picks one option silently;
    // - a choice with no options, or enumNames of the wrong length, draws a
    //   form that cannot be answered correctly;
    // - a malformed `required`, or an `enum` that is not a list, degrades to
    //   free text instead of being refused;
    // - an empty title or description is drawn as a blank.
    use super::*;
    use serde_json::{Value, json};

    fn probe_schema() -> Value {
        json!({"type": "object", "properties": {
            "colour": {"type": "string", "title": "Colour", "enum": ["red", "blue"]},
            "count": {"type": "integer", "title": "How many", "minimum": 1},
            "note": {"type": "string", "description": "free text", "format": "email"},
            "ok": {"type": "boolean", "default": true}},
            "required": ["colour"]})
    }

    fn names(fields: &[ElicitationField]) -> Vec<&str> {
        fields.iter().map(|f| f.name.as_str()).collect()
    }

    fn field(kind: ElicitationFieldKind, required: bool) -> ElicitationField {
        ElicitationField {
            name: "f".into(),
            title: "F".into(),
            description: None,
            required,
            kind,
            default: None,
        }
    }

    #[test]
    fn a_flat_schema_reads_every_primitive_kind() {
        let fields = elicitation_fields(&probe_schema(), None).unwrap();
        assert_eq!(names(&fields), ["colour", "count", "note", "ok"]);
        let colour = &fields[0];
        assert_eq!(colour.title, "Colour");
        assert!(colour.required);
        assert_eq!(
            colour.kind,
            ElicitationFieldKind::Choice(vec![
                ElicitationChoice {
                    value: "red".into(),
                    label: "red".into()
                },
                ElicitationChoice {
                    value: "blue".into(),
                    label: "blue".into()
                },
            ])
        );
        assert_eq!(fields[1].kind, ElicitationFieldKind::Integer);
        assert!(!fields[1].required);
        assert_eq!(
            fields[2].kind,
            ElicitationFieldKind::Text {
                format: Some("email".into())
            }
        );
        assert_eq!(fields[2].title, "note");
        assert_eq!(fields[2].description.as_deref(), Some("free text"));
        assert_eq!(fields[3].kind, ElicitationFieldKind::Boolean);
        assert_eq!(fields[3].default, Some(json!(true)));
    }

    #[test]
    fn schema_order_is_the_order_the_server_wrote() {
        let schema = json!({"type": "object", "properties": {
            "zeta": {"type": "string"}, "alpha": {"type": "string"}, "mid": {"type": "string"}}});
        let fields = elicitation_fields(&schema, None).unwrap();
        assert_eq!(names(&fields), ["zeta", "alpha", "mid"]);
    }

    #[test]
    fn property_order_wins_over_schema_order() {
        let order = ["ok".to_string(), "colour".to_string(), "ghost".to_string()];
        let fields = elicitation_fields(&probe_schema(), Some(&order)).unwrap();
        assert_eq!(names(&fields), ["ok", "colour", "count", "note"]);
    }

    #[test]
    fn one_of_const_titles_label_a_choice() {
        let schema = json!({"type": "object", "properties": {"size": {"type": "string",
            "oneOf": [{"const": "s", "title": "Small"}, {"const": "l", "title": "Large"}]}}});
        let fields = elicitation_fields(&schema, None).unwrap();
        assert_eq!(
            fields[0].kind,
            ElicitationFieldKind::Choice(vec![
                ElicitationChoice {
                    value: "s".into(),
                    label: "Small".into()
                },
                ElicitationChoice {
                    value: "l".into(),
                    label: "Large".into()
                },
            ])
        );
    }

    #[test]
    fn enum_names_label_an_enum() {
        let schema = json!({"type": "object", "properties": {"size": {"type": "string",
            "enum": ["s", "l"], "enumNames": ["Small", "Large"]}}});
        let fields = elicitation_fields(&schema, None).unwrap();
        assert_eq!(
            fields[0].kind,
            ElicitationFieldKind::Choice(vec![
                ElicitationChoice {
                    value: "s".into(),
                    label: "Small".into()
                },
                ElicitationChoice {
                    value: "l".into(),
                    label: "Large".into()
                },
            ])
        );
    }

    #[test]
    fn a_number_property_is_number() {
        let schema = json!({"type": "object", "properties": {"x": {"type": "number"}}});
        assert_eq!(
            elicitation_fields(&schema, None).unwrap()[0].kind,
            ElicitationFieldKind::Number
        );
    }

    #[test]
    fn an_empty_properties_form_is_valid_and_empty() {
        let schema = json!({"type": "object", "properties": {}});
        assert!(elicitation_fields(&schema, None).unwrap().is_empty());
    }

    #[test]
    fn schemas_outside_the_flat_subset_are_refused_with_a_reason() {
        let cases = [
            json!("nope"),
            json!({"type": "array", "items": {"type": "string"}}),
            json!({"type": "object", "properties": {"o": {"type": "object"}}}),
            json!({"type": "object", "properties": {"a": {"type": "array",
                "items": {"type": "string", "enum": ["x"]}}}}),
            json!({"type": "object", "properties": {"u": {"title": "no type"}}}),
            json!({"type": "object", "properties": {"a": {"type": "string"}}, "required": ["b"]}),
            json!({"type": "object", "properties": {"a": {"type": "string"}}, "required": "a"}),
            json!({"type": "object", "properties": {"a": {"type": "string"}}, "required": ["a", 1]}),
            json!({"type": "object", "properties": {"a": {"type": "string", "enum": []}}}),
            json!({"type": "object", "properties": {"a": {"type": "string", "oneOf": []}}}),
            json!({"type": "object", "properties": {"a": {"type": "string", "enum": "x"}}}),
            json!({"type": "object", "properties": {"a": {"type": "string", "oneOf": {}}}}),
            json!({"type": "object", "properties": {"a": {"type": "string",
                "enum": ["x", "y"], "enumNames": ["X"]}}}),
        ];
        for schema in cases {
            let err = elicitation_fields(&schema, None).expect_err(&schema.to_string());
            assert!(!err.is_empty(), "{schema}");
        }
    }

    #[test]
    fn text_is_trimmed() {
        let f = field(ElicitationFieldKind::Text { format: None }, true);
        assert_eq!(elicitation_value(&f, " hi ").unwrap(), Some(json!("hi")));
    }

    #[test]
    fn an_empty_answer_is_none_when_optional_and_an_error_when_required() {
        let optional = field(ElicitationFieldKind::Text { format: None }, false);
        assert_eq!(elicitation_value(&optional, "  ").unwrap(), None);
        let required = field(ElicitationFieldKind::Text { format: None }, true);
        assert!(elicitation_value(&required, "").is_err());
    }

    #[test]
    fn an_integer_is_a_whole_number() {
        let f = field(ElicitationFieldKind::Integer, true);
        assert_eq!(elicitation_value(&f, "2").unwrap(), Some(json!(2)));
        assert!(elicitation_value(&f, "2.5").is_err());
        assert!(elicitation_value(&f, "abc").is_err());
    }

    #[test]
    fn a_number_is_finite() {
        let f = field(ElicitationFieldKind::Number, true);
        assert_eq!(elicitation_value(&f, "2.5").unwrap(), Some(json!(2.5)));
        for bad in ["NaN", "inf", "-inf"] {
            assert!(elicitation_value(&f, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_boolean_reads_the_usual_words() {
        let f = field(ElicitationFieldKind::Boolean, true);
        for yes in ["true", "yes", "1"] {
            assert_eq!(
                elicitation_value(&f, yes).unwrap(),
                Some(json!(true)),
                "{yes}"
            );
        }
        for no in ["false", "no", "0"] {
            assert_eq!(
                elicitation_value(&f, no).unwrap(),
                Some(json!(false)),
                "{no}"
            );
        }
        assert!(elicitation_value(&f, "maybe").is_err());
    }

    #[test]
    fn a_choice_matches_a_value_or_a_label_and_nothing_else() {
        let f = field(
            ElicitationFieldKind::Choice(vec![ElicitationChoice {
                value: "s".into(),
                label: "Small".into(),
            }]),
            true,
        );
        assert_eq!(elicitation_value(&f, "s").unwrap(), Some(json!("s")));
        assert_eq!(elicitation_value(&f, "Small").unwrap(), Some(json!("s")));
        assert!(elicitation_value(&f, "Medium").is_err());
    }

    #[test]
    fn a_value_beats_an_earlier_options_label() {
        let schema = json!({"type": "object", "properties": {"n": {"type": "string",
            "enum": ["0", "1", "2"], "enumNames": ["1", "2", "3"]}}});
        let f = &elicitation_fields(&schema, None).unwrap()[0];
        assert_eq!(elicitation_value(f, "1").unwrap(), Some(json!("1")));
        assert_eq!(elicitation_value(f, "3").unwrap(), Some(json!("2")));
    }

    #[test]
    fn an_ambiguous_label_is_refused() {
        let f = field(
            ElicitationFieldKind::Choice(vec![
                ElicitationChoice {
                    value: "a".into(),
                    label: "Same".into(),
                },
                ElicitationChoice {
                    value: "b".into(),
                    label: "Same".into(),
                },
            ]),
            true,
        );
        assert!(elicitation_value(&f, "Same").is_err());
        assert_eq!(elicitation_value(&f, "b").unwrap(), Some(json!("b")));
    }

    #[test]
    fn empty_title_and_description_fall_back() {
        let schema = json!({"type": "object", "properties": {"q": {"type": "string",
            "title": "", "description": ""}}});
        let f = &elicitation_fields(&schema, None).unwrap()[0];
        assert_eq!(f.title, "q");
        assert_eq!(f.description, None);
    }
}
