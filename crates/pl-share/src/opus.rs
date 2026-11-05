//! Opus encoder for the stream's sound: 48 kHz stereo, 20 ms frames, about 96 kbit/s, in-band FEC.
//! `unsafe-libopus` is libopus transpiled to Rust (no C build, no CMake), BSD-3-Clause.

use unsafe_libopus::{
    OPUS_APPLICATION_AUDIO, OPUS_OK, OPUS_SET_BITRATE_REQUEST, OPUS_SET_COMPLEXITY_REQUEST,
    OPUS_SET_INBAND_FEC_REQUEST, OPUS_SET_PACKET_LOSS_PERC_REQUEST,
    OPUS_SET_VBR_CONSTRAINT_REQUEST, OPUS_SET_VBR_REQUEST, OpusEncoder, opus_encode,
    opus_encoder_create, opus_encoder_ctl, opus_encoder_destroy,
};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
/// 20 ms at 48 kHz.
pub const FRAME_SAMPLES: usize = 960;
/// Interleaved values per frame.
pub const FRAME_LEN: usize = FRAME_SAMPLES * CHANNELS;

pub struct Opus {
    enc: *mut OpusEncoder,
    out: Vec<u8>,
}

// SAFETY: the encoder state is plain memory owned by this value and used from one thread at a time.
unsafe impl Send for Opus {}

impl Opus {
    pub fn new(bitrate: i32) -> Result<Self, i32> {
        let mut err = 0;
        // SAFETY: valid arguments; the result is checked before use.
        let enc = unsafe {
            opus_encoder_create(
                SAMPLE_RATE as i32,
                CHANNELS as i32,
                OPUS_APPLICATION_AUDIO,
                &mut err,
            )
        };
        if enc.is_null() || err != OPUS_OK {
            return Err(err);
        }
        // SAFETY: `enc` is a live encoder; each request takes one i32.
        let results = unsafe {
            [
                opus_encoder_ctl!(enc, OPUS_SET_BITRATE_REQUEST, bitrate),
                // c2rust code has no SIMD: complexity 6 keeps a 20 ms frame well under 1 ms.
                opus_encoder_ctl!(enc, OPUS_SET_COMPLEXITY_REQUEST, 6),
                // Unconstrained VBR: quiet passages (and the keep-alive's silence) cost almost
                // nothing; 96 kbit/s is the average for music and game sound.
                opus_encoder_ctl!(enc, OPUS_SET_VBR_REQUEST, 1),
                opus_encoder_ctl!(enc, OPUS_SET_VBR_CONSTRAINT_REQUEST, 0),
                opus_encoder_ctl!(enc, OPUS_SET_INBAND_FEC_REQUEST, 1),
                opus_encoder_ctl!(enc, OPUS_SET_PACKET_LOSS_PERC_REQUEST, 5),
            ]
        };
        if let Some(&err) = results.iter().find(|&&r| r != OPUS_OK) {
            // SAFETY: created above, destroyed once.
            unsafe { opus_encoder_destroy(enc) };
            return Err(err);
        }
        Ok(Self {
            enc,
            out: vec![0; 1500],
        })
    }

    /// Encodes one 20 ms frame of interleaved stereo samples.
    pub fn encode(&mut self, pcm: &[i16; FRAME_LEN]) -> Option<Vec<u8>> {
        // SAFETY: `pcm` holds FRAME_SAMPLES per channel; `out` is writable for its length.
        let n = unsafe {
            opus_encode(
                self.enc,
                pcm.as_ptr(),
                FRAME_SAMPLES as i32,
                self.out.as_mut_ptr(),
                self.out.len() as i32,
            )
        };
        (n > 0).then(|| self.out[..n as usize].to_vec())
    }
}

impl Drop for Opus {
    fn drop(&mut self) {
        // SAFETY: created by opus_encoder_create, destroyed once.
        unsafe { opus_encoder_destroy(self.enc) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_silence_small_and_a_tone_bigger() {
        let mut opus = Opus::new(96_000).unwrap();
        let silence = [0i16; FRAME_LEN];
        let mut tone = [0i16; FRAME_LEN];
        for (i, s) in tone.iter_mut().enumerate() {
            let t = (i / CHANNELS) as f32 / SAMPLE_RATE as f32;
            *s = ((t * 1000.0 * std::f32::consts::TAU).sin() * 8000.0) as i16;
        }
        // Separate encoders: alternating frames would carry the tone's tail into the silence.
        let mut quiet_enc = Opus::new(96_000).unwrap();
        let quiet = (0..10)
            .map(|_| quiet_enc.encode(&silence).unwrap().len())
            .last()
            .unwrap();
        let loud = (0..10)
            .map(|_| opus.encode(&tone).unwrap().len())
            .last()
            .unwrap();
        assert!(quiet < loud / 4, "silence {quiet} B, tone {loud} B");
        assert!(loud < 1500);
    }

    #[test]
    fn bitrate_is_applied() {
        let mut tone = [0i16; FRAME_LEN];
        for (i, s) in tone.iter_mut().enumerate() {
            let t = (i / CHANNELS) as f32 / SAMPLE_RATE as f32;
            *s = ((t * 440.0 * std::f32::consts::TAU).sin() * 8000.0 + (t * 3100.0).sin() * 3000.0)
                as i16;
        }
        let size = |bitrate| {
            let mut opus = Opus::new(bitrate).unwrap();
            (0..50)
                .map(|_| opus.encode(&tone).unwrap().len())
                .sum::<usize>()
        };
        assert!(size(32_000) < size(96_000));
    }
}
