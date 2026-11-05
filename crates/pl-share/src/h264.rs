//! H.264 Annex B helpers (pure): NAL scanning, keyframe detection and making sure every keyframe
//! carries its SPS/PPS (some Media Foundation encoders put them only in the media type's
//! `MF_MT_MPEG_SEQUENCE_HEADER`, but a viewer joining later needs them in-band).

pub const NAL_IDR: u8 = 5;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;

/// The NAL units of an Annex B buffer (without start codes).
pub fn nals(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut pos = start_code_end(data, 0);
    std::iter::from_fn(move || {
        let start = pos?;
        let (end, next) = match find_start_code(data, start) {
            Some((at, len)) => (at, Some(at + len)),
            None => (data.len(), None),
        };
        pos = next;
        // Trailing zero bytes belong to the next start code (4-byte form) or are padding.
        let mut end = end;
        while end > start && data[end - 1] == 0 {
            end -= 1;
        }
        Some(&data[start..end])
    })
    .filter(|n| !n.is_empty())
}

pub fn nal_type(nal: &[u8]) -> u8 {
    nal.first().map_or(0, |b| b & 0x1f)
}

pub fn is_keyframe(data: &[u8]) -> bool {
    nals(data).any(|n| nal_type(n) == NAL_IDR)
}

pub fn has_parameter_sets(data: &[u8]) -> bool {
    let mut sps = false;
    let mut pps = false;
    for n in nals(data) {
        match nal_type(n) {
            NAL_SPS => sps = true,
            NAL_PPS => pps = true,
            _ => {}
        }
    }
    sps && pps
}

/// Keeps the SPS and PPS NAL units of `data` (Annex B) for later keyframes.
pub fn parameter_sets(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for n in nals(data) {
        if matches!(nal_type(n), NAL_SPS | NAL_PPS) {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(n);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `frame` with `sets` in front when it is a keyframe without its own parameter sets.
pub fn with_parameter_sets(frame: Vec<u8>, sets: Option<&[u8]>) -> Vec<u8> {
    match sets {
        Some(sets) if is_keyframe(&frame) && !has_parameter_sets(&frame) => {
            let mut out = Vec::with_capacity(sets.len() + frame.len());
            out.extend_from_slice(sets);
            out.extend_from_slice(&frame);
            out
        }
        _ => frame,
    }
}

/// Converts an avcC-style sequence header (`MF_MT_MPEG_SEQUENCE_HEADER` is Annex B on every
/// encoder seen so far, but be lenient) to Annex B.
pub fn sequence_header_to_annex_b(header: &[u8]) -> Option<Vec<u8>> {
    if start_code_end(header, 0).is_some() {
        return parameter_sets(header);
    }
    None
}

/// Position after the start code at or after `from`.
fn start_code_end(data: &[u8], from: usize) -> Option<usize> {
    find_start_code(data, from).map(|(at, len)| at + len)
}

/// The next `00 00 01` (3 bytes) at or after `from`: (index, length).
fn find_start_code(data: &[u8], from: usize) -> Option<(usize, usize)> {
    if data.len() < 3 {
        return None;
    }
    let mut i = from;
    while i + 2 < data.len() {
        if data[i + 2] > 1 {
            i += 3;
        } else if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            return Some((i, 3));
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPS: &[u8] = &[0x67, 0x42, 0xe0, 0x1f, 0xaa];
    const PPS: &[u8] = &[0x68, 0xce, 0x38, 0x80];
    const IDR: &[u8] = &[0x65, 0x88, 0x84, 0x00, 0x33];
    const P: &[u8] = &[0x41, 0x9a, 0x02, 0x00, 0x00, 0x05];

    fn annex_b(nals: &[&[u8]], four: bool) -> Vec<u8> {
        let mut out = Vec::new();
        for n in nals {
            if four {
                out.push(0);
            }
            out.extend_from_slice(&[0, 0, 1]);
            out.extend_from_slice(n);
        }
        out
    }

    #[test]
    fn scans_both_start_code_forms() {
        for four in [false, true] {
            let data = annex_b(&[SPS, PPS, IDR], four);
            let got: Vec<&[u8]> = nals(&data).collect();
            assert_eq!(got, vec![SPS, PPS, IDR], "four={four}");
        }
    }

    #[test]
    fn keeps_zero_bytes_inside_a_nal() {
        // P ends with 00 00 05: escaped data, not a start code.
        let data = annex_b(&[P], true);
        assert_eq!(nals(&data).collect::<Vec<_>>(), vec![P]);
        assert!(!is_keyframe(&data));
    }

    #[test]
    fn adds_parameter_sets_only_where_missing() {
        let sets = parameter_sets(&annex_b(&[SPS, PPS], true)).unwrap();
        let bare = annex_b(&[IDR], true);
        let fixed = with_parameter_sets(bare.clone(), Some(&sets));
        assert!(has_parameter_sets(&fixed) && is_keyframe(&fixed));
        assert_eq!(&fixed[sets.len()..], &bare[..]);

        let full = annex_b(&[SPS, PPS, IDR], false);
        assert_eq!(with_parameter_sets(full.clone(), Some(&sets)), full);
        let delta = annex_b(&[P], false);
        assert_eq!(with_parameter_sets(delta.clone(), Some(&sets)), delta);
    }

    #[test]
    fn sequence_header_in_annex_b() {
        let header = annex_b(&[SPS, PPS], true);
        assert_eq!(sequence_header_to_annex_b(&header), Some(header.clone()));
        assert_eq!(sequence_header_to_annex_b(&[1, 0x42, 0xe0]), None);
    }

    #[test]
    fn garbage_is_harmless() {
        assert_eq!(nals(&[]).count(), 0);
        assert_eq!(nals(&[0, 0]).count(), 0);
        assert!(!is_keyframe(&[1, 2, 3, 4, 5]));
    }
}
