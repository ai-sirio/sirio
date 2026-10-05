//! The control-plane messages of the host protocol (spec §6.2, §6.3). Every
//! struct ignores unknown fields and every enum that may grow degrades to an
//! `Other` variant, so a peer one minor behind still reads what a newer one
//! writes (spec §6.4).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct ProtocolVersion {
    pub major: u32,
    pub minor: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnknownMethod,
    InvalidParams,
    SessionsLive,
    StaleGeneration,
    HandshakeRequired,
    Internal,
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Event {
    pub subscription: u64,
    pub seq: u64,
    pub payload: Value,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Hello {
    pub client_version: String,
    pub majors: Vec<u32>,
    pub minor: u32,
}

/// The answer to `host.hello`, carried as the `result` of its Response. A new
/// reply type is a new major, so this enum is closed.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HelloReply {
    Welcome(Welcome),
    Refused(Refused),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Welcome {
    pub host_version: String,
    pub protocol: ProtocolVersion,
    pub capabilities: Vec<String>,
    pub host_id: String,
    pub generation: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Refused {
    pub reason: RefusedReason,
    pub host_major: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RefusedReason {
    NoCommonMajor,
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HostMode {
    OnDemand,
    Service,
    #[serde(other)]
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct HostInfo {
    pub version: String,
    pub pid: u32,
    pub mode: HostMode,
    pub sessions: u32,
    pub clients: u32,
    pub uptime_s: u64,
    pub generation: String,
    pub protocol: ProtocolVersion,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct HostStateEvent {
    pub clients: u32,
    pub sessions: u32,
    pub mode: HostMode,
    pub draining: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct ShutdownParams {
    #[serde(default)]
    pub force: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct HoldSessionParams {
    pub held: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct SubscribeParams {
    pub topic: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Subscribed {
    pub subscription: u64,
}

/// How a principal reached the host. Closed: a new transport is a major.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Local,
}

/// Who is asking. SP1 knows only the local owner, already verified by the
/// transport; SP7 adds the remote principals.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Principal {
    pub transport: Transport,
}

impl Response {
    pub fn ok(id: u64, result: impl Serialize) -> Self {
        Self {
            id,
            result: Some(serde_json::to_value(result).unwrap_or(Value::Null)),
            error: None,
        }
    }

    pub fn err(id: u64, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            id,
            result: None,
            error: Some(ErrorBody {
                code,
                message: message.into(),
            }),
        }
    }
}

pub mod method {
    pub const HELLO: &str = "host.hello";
    pub const PING: &str = "host.ping";
    pub const INFO: &str = "host.info";
    pub const SHUTDOWN: &str = "host.shutdown";
    pub const SUBSCRIBE: &str = "host.subscribe";
    pub const DEBUG_HOLD_SESSION: &str = "host.debug.hold_session";
}

pub mod topic {
    pub const HOST_STATE: &str = "host.state";
}

pub mod capability {
    pub const HOST_V1: &str = "host.v1";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_error_code_degrades_to_other() {
        let body: ErrorBody =
            serde_json::from_str(r#"{"code":"quota_exceeded","message":"x"}"#).unwrap();
        assert_eq!(body.code, ErrorCode::Other);
    }
    #[test]
    fn an_unknown_mode_and_refusal_degrade_to_other() {
        assert_eq!(
            serde_json::from_str::<HostMode>(r#""cluster""#).unwrap(),
            HostMode::Other
        );
        assert_eq!(
            serde_json::from_str::<RefusedReason>(r#""banned""#).unwrap(),
            RefusedReason::Other
        );
    }
    #[test]
    fn unknown_fields_from_a_newer_minor_are_ignored() {
        let w: Welcome = serde_json::from_str(
            r#"{"host_version":"1","protocol":{"major":1,"minor":9},
            "capabilities":["host.v1","future.v1"],"host_id":"h","generation":"g","region":"eu"}"#,
        )
        .unwrap();
        assert_eq!(w.protocol.minor, 9);
    }
    #[test]
    fn a_request_without_params_or_generation_decodes() {
        let r: Request = serde_json::from_str(r#"{"id":1,"method":"host.ping"}"#).unwrap();
        assert_eq!(r.params, serde_json::Value::Null);
        assert_eq!(r.generation, None);
    }
    #[test]
    fn hello_replies_are_tagged() {
        let json = serde_json::to_value(HelloReply::Refused(Refused {
            reason: RefusedReason::NoCommonMajor,
            host_major: 2,
        }))
        .unwrap();
        assert_eq!(json["type"], "refused");
        assert_eq!(json["reason"], "no_common_major");
    }
    #[test]
    fn shutdown_without_force_means_not_forced() {
        let p: ShutdownParams = serde_json::from_str("{}").unwrap();
        assert!(!p.force);
    }
}
