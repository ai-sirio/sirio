//! The host protocol as types and pure functions — no process, no socket.
//! Spec: docs/superpowers/specs/2026-10-05-host-foundation-design.md §6.

pub mod frame;
pub mod liveness;
pub mod messages;
pub mod paths;
pub mod state_file;
pub mod version;

/// The JSON Schema of every wire type, in one document. Every type, and every
/// type those reference, is an entry of the root `$defs`, so each
/// `#/$defs/<Name>` reference in it resolves inside the document. It is
/// committed as `protocol/host-v1/schema.json` and a test fails when the two
/// differ, so a change to the wire contract is made — and reviewed — on
/// purpose (spec §6.6).
pub fn wire_schema() -> serde_json::Value {
    use messages::{
        Event, Hello, HelloReply, HoldSessionParams, HostInfo, HostStateEvent, Request, Response,
        ShutdownParams, SubscribeParams, Subscribed,
    };
    use schemars::schema_for;
    use serde_json::Value;
    use state_file::HostStateFile;

    fn define(defs: &mut serde_json::Map<String, Value>, name: &str, schema: Value) {
        if let Some(existing) = defs.insert(name.to_string(), schema.clone()) {
            assert_eq!(existing, schema, "two different definitions of {name}");
        }
    }

    let mut defs = serde_json::Map::new();
    macro_rules! add {
        ($($t:ty),*) => {
            $({
                // schemars nests what a type references under that type's own
                // `$defs`; the references point at the document root, so the
                // nested definitions are hoisted there.
                let Value::Object(mut schema) =
                    serde_json::to_value(schema_for!($t)).expect("a schema serialises")
                else {
                    panic!("the schema of {} is an object", stringify!($t));
                };
                schema.remove("$schema");
                if let Some(Value::Object(nested)) = schema.remove("$defs") {
                    for (name, definition) in nested {
                        define(&mut defs, &name, definition);
                    }
                }
                define(&mut defs, stringify!($t), Value::Object(schema));
            })*
        };
    }
    add!(
        Request,
        Response,
        Event,
        Hello,
        HelloReply,
        HostInfo,
        HostStateEvent,
        ShutdownParams,
        HoldSessionParams,
        SubscribeParams,
        Subscribed,
        HostStateFile
    );
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "protocol": "sirio-host",
        "major": version::PROTOCOL_MAJOR,
        "minor": version::PROTOCOL_MINOR,
        "$defs": defs,
    })
}
