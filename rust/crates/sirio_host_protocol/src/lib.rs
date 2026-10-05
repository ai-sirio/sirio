//! The host protocol as types and pure functions — no process, no socket.
//! Spec: docs/superpowers/specs/2026-10-05-host-foundation-design.md §6.

pub mod frame;
pub mod liveness;
pub mod messages;
pub mod paths;
pub mod state_file;
pub mod version;

/// The JSON Schema of every wire type, in one document. It is committed as
/// `protocol/host-v1/schema.json` and a test fails when the two differ, so a
/// change to the wire contract is made — and reviewed — on purpose
/// (spec §6.6).
pub fn wire_schema() -> serde_json::Value {
    use messages::{
        Event, Hello, HelloReply, HoldSessionParams, HostInfo, HostStateEvent, Request, Response,
        ShutdownParams, SubscribeParams, Subscribed,
    };
    use schemars::schema_for;
    use state_file::HostStateFile;

    let mut defs = serde_json::Map::new();
    macro_rules! add {
        ($($t:ty),*) => {
            $(defs.insert(
                stringify!($t).to_string(),
                serde_json::to_value(schema_for!($t)).expect("a schema serialises"),
            );)*
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
        "protocol": "sirio-host",
        "major": version::PROTOCOL_MAJOR,
        "minor": version::PROTOCOL_MINOR,
        "$defs": defs,
    })
}
