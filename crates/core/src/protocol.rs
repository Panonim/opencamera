//! Messages the phone and desktop send each other to set up a connection, matching the phone website's copy.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CameraInfo {
    pub device_id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facing: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhoneSettingsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facing_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_enabled: Option<bool>,
    /// Either "auto" or a number of kbps.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_saver: Option<bool>,
}

/// Phone status passed to the desktop window unchanged.
pub type PhoneState = serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PhoneStats {
    pub bitrate_kbps: f64,
    pub encode_fps: f64,
    pub rtt_ms: Option<f64>,
    pub quality_limitation: String,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub encoder: Option<String>,
    #[serde(default)]
    pub hardware: Option<bool>,
    #[serde(default)]
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IceServer {
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IceCandidate {
    pub candidate: String,
    #[serde(default)]
    pub sdp_mid: Option<String>,
    #[serde(default, rename = "sdpMLineIndex")]
    pub sdp_mline_index: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PhoneMessage {
    #[serde(rename_all = "camelCase")]
    Hello {
        protocol_version: u32,
        token: Option<String>,
        device_id: Option<String>,
        device_secret: Option<String>,
        device_name: String,
        user_agent: String,
    },
    Offer { sdp: String },
    Ice { candidate: Option<IceCandidate> },
    State { state: PhoneState },
    Stats { stats: PhoneStats },
    Bye,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RejectReason {
    InvalidToken,
    ExpiredToken,
    UnknownDevice,
    Busy,
    VersionMismatch,
    Replaced,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DesktopMessage {
    #[serde(rename_all = "camelCase")]
    Welcome {
        device_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        device_secret: Option<String>,
        desktop_name: String,
        ice_servers: Vec<IceServer>,
    },
    Reject { reason: RejectReason },
    Answer { sdp: String },
    Ice { candidate: Option<IceCandidate> },
    Control { settings: PhoneSettingsPatch },
    Bye { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hello_from_phone() {
        let msg: PhoneMessage = serde_json::from_str(
            r#"{"type":"hello","protocolVersion":1,"token":"abc","deviceName":"iPhone","userAgent":"UA"}"#,
        )
        .unwrap();
        assert!(matches!(msg, PhoneMessage::Hello { token: Some(t), device_id: None, .. } if t == "abc"));
    }

    #[test]
    fn parses_end_of_candidates() {
        let msg: PhoneMessage = serde_json::from_str(r#"{"type":"ice","candidate":null}"#).unwrap();
        assert_eq!(msg, PhoneMessage::Ice { candidate: None });
    }

    #[test]
    fn serializes_desktop_messages_in_ts_shape() {
        let json = serde_json::to_value(DesktopMessage::Reject { reason: RejectReason::ExpiredToken }).unwrap();
        assert_eq!(json, serde_json::json!({"type":"reject","reason":"expired_token"}));
        let json = serde_json::to_value(DesktopMessage::Control {
            settings: PhoneSettingsPatch { video_enabled: Some(false), ..Default::default() },
        })
        .unwrap();
        assert_eq!(json, serde_json::json!({"type":"control","settings":{"videoEnabled":false}}));
    }

    #[test]
    fn bitrate_patch_accepts_auto_or_number() {
        let auto: PhoneSettingsPatch = serde_json::from_str(r#"{"bitrate":"auto"}"#).unwrap();
        assert_eq!(auto.bitrate, Some(serde_json::json!("auto")));
        let manual: PhoneSettingsPatch = serde_json::from_str(r#"{"bitrate":3000,"batterySaver":false}"#).unwrap();
        assert_eq!(serde_json::to_value(manual).unwrap(), serde_json::json!({"bitrate":3000,"batterySaver":false}));
    }

    #[test]
    fn parses_stats_from_older_phones() {
        let msg: PhoneMessage = serde_json::from_str(
            r#"{"type":"stats","stats":{"bitrateKbps":1,"encodeFps":30,"rttMs":null,"qualityLimitation":"none"}}"#,
        )
        .unwrap();
        assert!(matches!(msg, PhoneMessage::Stats { stats } if stats.profile.is_none() && stats.scale.is_none()));
    }

    #[test]
    fn rejects_unknown_messages() {
        assert!(serde_json::from_str::<PhoneMessage>(r#"{"type":"evil"}"#).is_err());
    }
}
