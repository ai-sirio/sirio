use crate::messages::{HostMode, ProtocolVersion};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, schemars::JsonSchema)]
pub struct HostStateFile {
    pub pid: u32,
    pub start_time: u64,
    pub version: String,
    pub protocol: ProtocolVersion,
    pub mode: HostMode,
    pub endpoint: String,
    pub generation: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_file_from_a_newer_minor_still_reads() {
        let s: HostStateFile = serde_json::from_str(
            r#"{"pid":7,"start_time":9,"version":"0.40.0",
        "protocol":{"major":1,"minor":4},"mode":"service","endpoint":"/e","generation":"g","tls":true}"#,
        )
        .unwrap();
        assert_eq!(s.pid, 7);
    }
}
