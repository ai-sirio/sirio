//! The wire contract is committed. A change to any wire type changes this
//! schema, and the change has to be committed — and reviewed — on purpose:
//!   cargo run -p sirio_host_protocol --example write_schema -- ../protocol/host-v1/schema.json

#[test]
fn the_committed_schema_matches_the_wire_types() {
    let committed = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../protocol/host-v1/schema.json"
    ))
    .expect("protocol/host-v1/schema.json");
    let committed: serde_json::Value = serde_json::from_str(&committed).unwrap();
    assert_eq!(
        committed,
        sirio_host_protocol::wire_schema(),
        "wire types changed: regenerate protocol/host-v1/schema.json (see this file's header) and review the diff"
    );
}

#[test]
fn every_ledger_capability_names_methods_the_protocol_defines() {
    let ledger: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../protocol/host-v1/capabilities.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let known = [
        sirio_host_protocol::messages::method::HELLO,
        sirio_host_protocol::messages::method::PING,
        sirio_host_protocol::messages::method::INFO,
        sirio_host_protocol::messages::method::SHUTDOWN,
        sirio_host_protocol::messages::method::SUBSCRIBE,
        sirio_host_protocol::messages::topic::HOST_STATE,
    ];
    for cap in ledger["capabilities"].as_array().unwrap() {
        for name in cap["methods"]
            .as_array()
            .unwrap()
            .iter()
            .chain(cap["topics"].as_array().unwrap())
        {
            assert!(
                known.contains(&name.as_str().unwrap()),
                "ledger names {name}, which the protocol does not define"
            );
        }
    }
}
