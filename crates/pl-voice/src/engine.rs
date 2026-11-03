//! The boundary between the launcher and its voice page (`voice.html` in the hidden `voice`
//! window; VOICE-L2 builds the engine behind it).
//!
//! - [`EngineCommand`]: launcher → page, through the channel the page hands over with
//!   `voice_page_attach` (only the launcher can write to it: no other window can forge one).
//! - [`PageReport`]: page → launcher, the `voice_page_report` command; only the voice window may
//!   call it ([`crate::guard`]), and the voice window may call nothing else.
//!
//! The page holds no state the launcher needs back after a crash: the launcher opens the window
//! again, attaches, configures and joins anew (a `join` on the same room socket replaces the SFU
//! session). SDP, TURN credentials and device ids never reach a log line ([`crate::wire::Sdp`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::gain::Playout;
use crate::view::{VoiceDevices, VoiceMic};

/// How the page processes and sends the microphone, and plays people.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineSettings {
    /// None: the system's default.
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub noise_suppression: bool,
    pub echo_cancellation: bool,
    pub auto_gain: bool,
    /// None: automatic (RNNoise's speech probability and an adapting noise floor); else 0–100.
    pub sensitivity: Option<u8>,
    /// Percent (auto gain off).
    pub mic_volume: u16,
    /// Push-to-talk: the gate follows [`EngineCommand::Talk`] only; voice activity is not used.
    pub push_to_talk: bool,
    /// Opus bit rate (kbps, Studio's `voiceKbps`).
    pub kbps: u16,
    /// "Sesli sohbet sesleri" (the launcher sends only the sounds it wants anyway).
    pub sounds: bool,
    /// "Ses seviyesi" (VOICE-CALL): everyone's voice at once, on top of each person's own;
    /// percent, held to the playout path's top (`pl_voice::gain`).
    pub output_volume: u16,
}

/// One person as the page plays them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EnginePerson {
    pub id: String,
    /// Percent, held to the page's top (`pl_voice::gain`).
    pub volume: u16,
    /// Not played: muted for the player, or muted for everyone.
    pub silent: bool,
}

/// TURN credentials for the peer connection (only after ICE failed; never logged).
#[derive(Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineIce {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

impl std::fmt::Debug for EngineIce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<ice {} urls>", self.urls.len())
    }
}

/// A short sound the page plays in its own output (the ring itself is the launcher's). One family
/// of our own synthesized notes (VOICE-CALL): the launcher decides when, so a press on the page, a
/// shortcut, the tray and the in-game overlay sound the same; "Sesli sohbet sesleri" turns all
/// but the waiting tone off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EngineSound {
    /// Someone came into the room.
    Join,
    /// Someone left.
    Leave,
    /// The player's microphone turned off (by the player, or a moderator's mute).
    MicOff,
    /// The player's microphone turned on.
    MicOn,
    /// The player deafened.
    DeafOn,
    /// The player's sound is back (undeafened).
    DeafOff,
    /// The player is in: the first `hello` with someone there, a call answered, or the connection
    /// back after "Yeniden bağlanıyor…".
    Connected,
    /// The stay ended (the player left, or the room ended it); never on a move to another room.
    Ended,
    /// The connection dropped ("Yeniden bağlanıyor…" shows).
    Lost,
    /// The caller's waiting tone, until [`EngineSound::Stop`] or someone comes.
    Waiting,
    Stop,
}

/// Launcher → page.
#[derive(Clone, PartialEq, Serialize, specta::Type)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum EngineCommand {
    /// How to capture and play; again whenever the settings change.
    Configure {
        settings: EngineSettings,
    },
    /// Start a stay: open the microphone (if it can) and make a peer connection with one
    /// send-only audio track; answer with [`PageReport::Offer`].
    Join,
    /// The room's answer to the offer (`hello.sdp`).
    Answer {
        sdp: String,
    },
    /// The room's offer (people to receive, or receivers closed; `mids`: mid → person): answer
    /// with [`PageReport::Answer`].
    Offer {
        sdp: String,
        mids: BTreeMap<String, String>,
    },
    /// Whether the microphone may send now (not muted, not deafened, not muted for everyone, and
    /// push-to-talk held when it is on). The page's gate also needs speech in voice activity.
    Talk {
        open: bool,
        /// Push-to-talk was let go: a closing gate keeps its 200 ms tail (the end of a word). False:
        /// a mute, a deafen or a moderator's mute closes it at once (a 5 ms fade).
        tail: bool,
    },
    /// Deafened: nothing plays.
    Deaf {
        on: bool,
    },
    /// How each person plays, whole (after every change).
    People {
        people: Vec<EnginePerson>,
    },
    /// TURN credentials: restart ICE with them.
    Ice {
        ice: EngineIce,
    },
    Sound {
        sound: EngineSound,
    },
    /// Report [`PageReport::Stats`] once (the quality mark asks every 2 s only while shown).
    Stats,
    /// The settings page: devices and the microphone's level without a call (`on`), or not.
    Setup {
        on: bool,
    },
    /// "Mikrofonu dene": the player hears themselves (nothing goes anywhere), or not.
    SelfTest {
        on: bool,
    },
    /// The launcher's live meters (VOICE-CALL; only while its window shows them): report
    /// [`PageReport::Levels`] at most sixteen times a second, only on change. `me`: the player's
    /// microphone (the capture listens for it even while nothing is sent); `people`: who makes
    /// sound in the room. A stay's end turns both off.
    Meters {
        me: bool,
        people: bool,
    },
    /// The stay ended: close the peer connection, stop the microphone.
    Leave,
}

impl std::fmt::Debug for EngineCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // SDP and credentials stay out of logs.
        let kind = match self {
            Self::Configure { .. } => "configure",
            Self::Join => "join",
            Self::Answer { .. } => "answer",
            Self::Offer { .. } => "offer",
            Self::Talk { .. } => "talk",
            Self::Deaf { .. } => "deaf",
            Self::People { .. } => "people",
            Self::Ice { .. } => "ice",
            Self::Sound { .. } => "sound",
            Self::Stats => "stats",
            Self::Setup { .. } => "setup",
            Self::SelfTest { .. } => "selfTest",
            Self::Meters { .. } => "meters",
            Self::Leave => "leave",
        };
        f.write_str(kind)
    }
}

/// Page → launcher.
#[derive(Clone, PartialEq, Deserialize, specta::Type)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum PageReport {
    /// The page runs and listens (once per load, after attaching).
    Ready,
    /// The join offer (`sdp` none: no microphone, listen only); `mid` the audio m-line's.
    Offer {
        sdp: Option<String>,
        mid: Option<String>,
    },
    /// The answer to the room's offer.
    Answer { sdp: String },
    /// Who speaks now (people's ids) and whether the player's microphone sends; at most ten a
    /// second, only on change.
    Speaking { ids: Vec<String>, me: bool },
    /// The connection's quality changed: 0 good, 1 weak, 2 bad.
    Quality { q: u8 },
    /// ICE did not connect within 5 s: the launcher asks the room for TURN.
    NeedTurn,
    /// ICE was lost and did not come back within 3 s: the launcher joins anew (a fresh offer).
    Lost,
    /// The audio connection works (again).
    Connected,
    /// The microphone's state (and `changed`: the chosen one went, the default took its place).
    Mic { mic: VoiceMic, changed: bool },
    /// The devices (on start, and when they change).
    Devices { devices: VoiceDevices },
    /// The microphone's level for the settings page (0–1, at most ten a second, only in setup).
    Level { level: f32, open: bool },
    /// The live meters' levels (VOICE-CALL, after [`EngineCommand::Meters`]): the player's
    /// microphone (`me`, 0–1 on the settings bar's scale; `open`: sent now) and the people who make
    /// sound now (`ids` with their `levels`, 0–1; the quiet left out).
    Levels {
        me: f32,
        open: bool,
        ids: Vec<String>,
        levels: Vec<f32>,
    },
    /// Answer to [`EngineCommand::Stats`].
    #[serde(rename_all = "camelCase")]
    Stats {
        rtt_ms: Option<f32>,
        loss: Option<f32>,
        jitter_ms: Option<f32>,
    },
    /// How people play (once when the sound opens, and when the path changes): the volume
    /// slider's top follows it ([`crate::gain`]).
    Playout { playout: Playout },
    /// Something failed that the launcher should know of (a short code, no detail).
    Failed { code: String },
}

impl std::fmt::Debug for PageReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Offer { sdp, .. } => write!(f, "offer(sdp: {})", sdp.is_some()),
            Self::Answer { .. } => f.write_str("answer"),
            Self::Speaking { ids, me } => write!(f, "speaking({} people, me: {me})", ids.len()),
            Self::Quality { q } => write!(f, "quality({q})"),
            Self::Mic { mic, changed } => write!(f, "mic({mic:?}, changed: {changed})"),
            Self::Devices { devices } => write!(
                f,
                "devices({} in, {} out)",
                devices.inputs.len(),
                devices.outputs.len()
            ),
            Self::Level { .. } => f.write_str("level"),
            Self::Levels { ids, .. } => write!(f, "levels({} people)", ids.len()),
            Self::Stats { .. } => f.write_str("stats"),
            Self::Playout { playout } => write!(f, "playout({playout:?})"),
            Self::Failed { code } => write!(f, "failed({code})"),
            Self::Ready => f.write_str("ready"),
            Self::NeedTurn => f.write_str("needTurn"),
            Self::Lost => f.write_str("lost"),
            Self::Connected => f.write_str("connected"),
        }
    }
}

impl PageReport {
    /// A report the page sent that is too big to be honest (SDP has a 16 KB ceiling on the room
    /// socket; nobody has 64 devices).
    pub fn is_oversized(&self) -> bool {
        const SDP_MAX: usize = crate::wire::limits::MAX_FRAME_CHARS;
        let device_too_big =
            |device: &crate::view::VoiceDevice| device.id.len() > 256 || device.label.len() > 256;
        match self {
            Self::Offer { sdp, mid } => {
                sdp.as_ref().is_some_and(|sdp| sdp.len() > SDP_MAX)
                    || mid.as_ref().is_some_and(|mid| mid.len() > 32)
            }
            Self::Answer { sdp } => sdp.len() > SDP_MAX,
            Self::Speaking { ids, .. } => ids.len() > 64 || ids.iter().any(|id| id.len() > 64),
            Self::Devices { devices } => {
                devices.inputs.len() + devices.outputs.len() > 64
                    || devices
                        .inputs
                        .iter()
                        .chain(&devices.outputs)
                        .any(device_too_big)
            }
            Self::Level { level, .. } => !level.is_finite(),
            Self::Levels {
                me, ids, levels, ..
            } => {
                !me.is_finite()
                    || ids.len() > 64
                    || ids.len() != levels.len()
                    || ids.iter().any(|id| id.len() > 64)
                    || levels.iter().any(|level| !level.is_finite())
            }
            Self::Failed { code } => code.len() > 64,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn commands_as_the_page_reads_them() {
        assert_eq!(
            serde_json::to_value(EngineCommand::Talk {
                open: true,
                tail: false
            })
            .unwrap(),
            json!({"t":"talk","open":true,"tail":false})
        );
        assert_eq!(
            serde_json::to_value(EngineCommand::Talk {
                open: false,
                tail: true
            })
            .unwrap(),
            json!({"t":"talk","open":false,"tail":true})
        );
        assert_eq!(
            serde_json::to_value(EngineCommand::Offer {
                sdp: "v=0".into(),
                mids: BTreeMap::from([("1".into(), "u2".into())])
            })
            .unwrap(),
            json!({"t":"offer","sdp":"v=0","mids":{"1":"u2"}})
        );
        assert_eq!(
            serde_json::to_value(EngineCommand::Sound {
                sound: EngineSound::Waiting
            })
            .unwrap(),
            json!({"t":"sound","sound":"waiting"})
        );
        // VOICE-CALL: the sounds' names and the meters as the page reads them.
        assert_eq!(
            serde_json::to_value(EngineCommand::Sound {
                sound: EngineSound::MicOff
            })
            .unwrap(),
            json!({"t":"sound","sound":"micOff"})
        );
        assert_eq!(
            serde_json::to_value(EngineCommand::Meters {
                me: true,
                people: false
            })
            .unwrap(),
            json!({"t":"meters","me":true,"people":false})
        );
        assert_eq!(
            format!(
                "{:?}",
                EngineCommand::Answer {
                    sdp: "secret".into()
                }
            ),
            "answer"
        );
    }

    #[test]
    fn reports_as_the_page_writes_them() {
        let offer: PageReport =
            serde_json::from_value(json!({"t":"offer","sdp":"v=0","mid":"0"})).unwrap();
        assert!(matches!(offer, PageReport::Offer { sdp: Some(_), .. }));
        let listen: PageReport =
            serde_json::from_value(json!({"t":"offer","sdp":null,"mid":null})).unwrap();
        assert!(matches!(listen, PageReport::Offer { sdp: None, .. }));
        let speaking: PageReport =
            serde_json::from_value(json!({"t":"speaking","ids":["u2"],"me":true})).unwrap();
        assert_eq!(format!("{speaking:?}"), "speaking(1 people, me: true)");
        let mic: PageReport =
            serde_json::from_value(json!({"t":"mic","mic":"denied","changed":false})).unwrap();
        assert!(matches!(
            mic,
            PageReport::Mic {
                mic: VoiceMic::Denied,
                ..
            }
        ));
        let stats: PageReport =
            serde_json::from_value(json!({"t":"stats","rttMs":40.5,"loss":0.01,"jitterMs":3}))
                .unwrap();
        assert!(matches!(
            stats,
            PageReport::Stats {
                rtt_ms: Some(_),
                ..
            }
        ));
        // L2's page sends the playout path as `pl_voice::gain::Playout` names it.
        let playout: PageReport =
            serde_json::from_value(json!({"t":"playout","playout":"element"})).unwrap();
        assert_eq!(
            playout,
            PageReport::Playout {
                playout: Playout::Element
            }
        );
        assert_eq!(format!("{playout:?}"), "playout(Element)");
        assert!(
            serde_json::from_value::<PageReport>(json!({"t":"playout","playout":"loud"})).is_err()
        );
        assert!(serde_json::from_value::<PageReport>(json!({"t":"whatever"})).is_err());
        let big = PageReport::Answer {
            sdp: "x".repeat(20_000),
        };
        assert!(big.is_oversized());
        assert!(!format!("{big:?}").contains('x'));
        // VOICE-CALL's meters: names and levels pair up, nothing endless or huge.
        let levels: PageReport = serde_json::from_value(
            json!({"t":"levels","me":0.5,"open":true,"ids":["u2"],"levels":[0.25]}),
        )
        .unwrap();
        assert!(!levels.is_oversized());
        assert_eq!(format!("{levels:?}"), "levels(1 people)");
        let unpaired: PageReport = serde_json::from_value(
            json!({"t":"levels","me":0.5,"open":false,"ids":["u2","u3"],"levels":[0.25]}),
        )
        .unwrap();
        assert!(unpaired.is_oversized());
        let crowd = PageReport::Levels {
            me: 0.0,
            open: false,
            ids: (0..65).map(|n| n.to_string()).collect(),
            levels: vec![0.5; 65],
        };
        assert!(crowd.is_oversized());
        let endless = PageReport::Levels {
            me: f32::NAN,
            open: false,
            ids: Vec::new(),
            levels: Vec::new(),
        };
        assert!(endless.is_oversized());
    }
}
