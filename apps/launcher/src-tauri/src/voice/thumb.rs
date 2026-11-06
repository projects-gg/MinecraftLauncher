//! SHARE-LIVE: a live share's still picture for the hover card of the launcher's member lists
//! (`share_thumb`) and of the in-game overlay's (`pl_overlay::Overlay::on_thumb`). The player's own
//! come from this computer (the publisher's newest picture); anyone else's from the friends
//! service, which also says when the player may not see it (`locked`: the room is there, joining
//! it is not). An answer is kept 20 s (the service's own `max-age`), by room, sharer, share and the
//! time of the room's newest picture, so a pointer sweeping a list asks once. Nothing is written
//! to disk.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine as _;
use pl_social::{SocialService, VoiceThumb};
use pl_voice::Voice;
use serde::Serialize;

/// How long an answer is kept (the service's `Cache-Control: max-age`).
const KEEP: Duration = Duration::from_secs(20);
/// A failed request is not asked again for this long (no connection, the service's bucket).
const KEEP_FAILED: Duration = Duration::from_secs(5);
/// Answers kept at most (a long member list swept with the pointer).
const KEPT_MAX: usize = 48;

/// What the hover card shows (`share_thumb`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ShareThumb {
    /// The picture, `data:image/jpeg;base64,…`.
    Image {
        #[serde(rename = "dataUrl")]
        data_url: String,
    },
    /// The player may not join that room: a blurred lock, no picture.
    Locked,
    /// No picture yet, or nothing to show.
    None,
}

impl ShareThumb {
    fn of(answer: &VoiceThumb) -> Self {
        match answer {
            VoiceThumb::Image(jpg) => Self::Image {
                data_url: format!(
                    "data:image/jpeg;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(jpg)
                ),
            },
            VoiceThumb::Locked => Self::Locked,
            VoiceThumb::None => Self::None,
        }
    }
}

/// (room, sharer, share, the room's newest picture's time) → (until when, the answer).
type Key = (String, String, u8, Option<i64>);
static KEPT: Mutex<Vec<(Key, Instant, VoiceThumb)>> = Mutex::new(Vec::new());
/// The player's own newest pictures, by share.
static OWN: Mutex<Vec<(u8, Vec<u8>)>> = Mutex::new(Vec::new());

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The publisher made a picture of the player's share `n` (it goes to the room too).
pub(crate) fn keep_own(n: u8, jpg: &[u8]) {
    let mut own = lock(&OWN);
    own.retain(|(kept, _)| *kept != n);
    own.push((n, jpg.to_vec()));
}

/// The player's share `n` ended: its picture goes.
pub(crate) fn forget_own(n: u8) {
    lock(&OWN).retain(|(kept, _)| *kept != n);
}

/// The page's answer.
pub(crate) async fn for_page(
    social: &SocialService,
    voice: &Voice,
    room: &str,
    user: &str,
    n: u8,
) -> ShareThumb {
    ShareThumb::of(&fetch(social, voice, room, user, n).await)
}

/// Share `n` of `user` in `room`: the player's own (their id, or the overlay's `@me`) from this
/// computer, anyone else's from the friends service (kept a while).
pub(crate) async fn fetch(
    social: &SocialService,
    voice: &Voice,
    room: &str,
    user: &str,
    n: u8,
) -> VoiceThumb {
    let me = super::my_id();
    if user == pl_overlay::voice::ME || (!me.is_empty() && user == me) {
        return lock(&OWN)
            .iter()
            .find(|(kept, _)| *kept == n)
            .map_or(VoiceThumb::None, |(_, jpg)| VoiceThumb::Image(jpg.clone()));
    }
    // The room's newest picture's time, when the player is in that room (a crew room's list says
    // only who is live): a newer picture is asked for at once.
    let th = {
        let state = voice.state();
        state
            .room
            .as_ref()
            .filter(|mine| mine.id == room)
            .and_then(|_| {
                state
                    .share
                    .live
                    .iter()
                    .find(|share| share.person == user && share.n == n)
            })
            .and_then(|share| share.th)
    };
    let key: Key = (room.to_owned(), user.to_owned(), n, th);
    let now = Instant::now();
    {
        let mut kept = lock(&KEPT);
        kept.retain(|(_, until, _)| *until > now);
        if let Some((_, _, answer)) = kept.iter().find(|(at, _, _)| *at == key) {
            return answer.clone();
        }
    }
    let (answer, keep) = match social.voice_thumb(room, user, n).await {
        Ok(answer) => (answer, KEEP),
        Err(error) => {
            tracing::debug!(%error, "share: no still picture");
            (VoiceThumb::None, KEEP_FAILED)
        }
    };
    let mut kept = lock(&KEPT);
    kept.retain(|(at, _, _)| *at != key);
    if kept.len() >= KEPT_MAX {
        kept.remove(0);
    }
    kept.push((key, Instant::now() + keep, answer.clone()));
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_reads_the_three_answers() {
        assert_eq!(
            serde_json::to_value(ShareThumb::of(&VoiceThumb::Image(vec![0xFF, 0xD8, 0xFF])))
                .unwrap(),
            serde_json::json!({"kind": "image", "dataUrl": "data:image/jpeg;base64,/9j/"})
        );
        assert_eq!(
            serde_json::to_value(ShareThumb::Locked).unwrap(),
            serde_json::json!({"kind": "locked"})
        );
        assert_eq!(
            serde_json::to_value(ShareThumb::None).unwrap(),
            serde_json::json!({"kind": "none"})
        );
        keep_own(1, &[1, 2]);
        keep_own(1, &[3]);
        assert_eq!(lock(&OWN).as_slice(), [(1, vec![3])]);
        forget_own(1);
        assert!(lock(&OWN).is_empty());
    }
}
