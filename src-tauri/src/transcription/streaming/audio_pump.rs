//! Online (streaming) audio resampler + PCM16 conversion.
//!
//! `OnlineResampler` carries a fractional source position and one trailing
//! sample across `process()` calls so chunked input matches the offline
//! `resample()` in `audio/recorder.rs` within ±2 samples + 0.01 amplitude.

pub struct OnlineResampler {
    ratio: f64,            // from_rate / to_rate
    src_offset: f64,       // fractional position into the next input chunk
    trailing: Option<f32>, // last sample from prior chunk for interpolation
}

impl OnlineResampler {
    pub fn new(from_rate: u32, to_rate: u32) -> Self {
        Self {
            ratio: from_rate as f64 / to_rate as f64,
            src_offset: 0.0,
            trailing: None,
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        // Identity path
        if (self.ratio - 1.0).abs() < 1e-9 && self.trailing.is_none() && self.src_offset == 0.0 {
            return input.to_vec();
        }

        // Conceptual stream: [trailing?, input[0], input[1], ...]
        let leading_count = if self.trailing.is_some() { 1 } else { 0 };
        let total_len = leading_count + input.len();
        let sample_at = |idx: usize| -> f32 {
            if let Some(t) = self.trailing {
                if idx == 0 {
                    return t;
                }
                input[idx - 1]
            } else {
                input[idx]
            }
        };

        let mut out = Vec::new();
        let mut pos = self.src_offset;
        loop {
            let base = pos as usize;
            // Need base+1 to interpolate; if not yet available, stop
            if base + 1 >= total_len {
                break;
            }
            let frac = pos - base as f64;
            let a = sample_at(base);
            let b = sample_at(base + 1);
            out.push((a as f64 * (1.0 - frac) + b as f64 * frac) as f32);
            pos += self.ratio;
        }

        // Save state: re-anchor `src_offset` to be measured from "start of next chunk"
        // with `trailing = last sample of current input`.
        self.src_offset = pos - (total_len - 1) as f64;
        self.trailing = Some(input[input.len() - 1]);

        out
    }

    /// Emit any remaining trailing sample once the stream ends.
    pub fn flush(&mut self) -> Vec<f32> {
        if self.src_offset < 0.5 {
            if let Some(t) = self.trailing.take() {
                return vec![t];
            }
        }
        self.trailing = None;
        Vec::new()
    }
}

/// Convert f32 audio samples in [-1.0, 1.0] to PCM16. NaN → 0, ±Inf → clamp.
pub fn f32_to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| {
            if s.is_nan() {
                0
            } else if s == f32::INFINITY {
                i16::MAX
            } else if s == f32::NEG_INFINITY {
                i16::MIN
            } else if s >= 1.0 {
                i16::MAX
            } else if s <= -1.0 {
                -i16::MAX
            } else {
                (s * i16::MAX as f32) as i16
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resampling in chunks must produce the same output as resampling the whole buffer at once.
    #[test]
    fn online_resampler_matches_offline_for_sine_in_chunks() {
        let from_rate = 48_000;
        let to_rate = 16_000;
        let n_samples = 4800;
        let input: Vec<f32> = (0..n_samples)
            .map(|i| (i as f32 * 0.05).sin())
            .collect();

        // Reference: offline resample the whole thing
        let reference = crate::audio::recorder::resample_for_tests(&input, from_rate, to_rate);

        // Streaming: feed in 5 chunks of 960 samples each
        let mut resampler = OnlineResampler::new(from_rate, to_rate);
        let mut online = Vec::new();
        for chunk in input.chunks(960) {
            online.extend(resampler.process(chunk));
        }
        online.extend(resampler.flush());

        // Allow ±2 sample length drift from fractional accumulation
        assert!(
            (online.len() as i64 - reference.len() as i64).abs() <= 2,
            "online {} vs offline {}",
            online.len(),
            reference.len(),
        );
        // Compare overlapping samples within tolerance
        let n = online.len().min(reference.len());
        for i in 0..n {
            let diff = (online[i] as f64 - reference[i] as f64).abs();
            assert!(diff < 0.01, "sample {} diverged: online {} ref {}", i, online[i], reference[i]);
        }
    }

    #[test]
    fn online_resampler_passthrough_when_rates_match() {
        let mut r = OnlineResampler::new(16_000, 16_000);
        let out = r.process(&[0.1, 0.2, 0.3]);
        assert_eq!(out, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn online_resampler_handles_empty_chunk() {
        let mut r = OnlineResampler::new(48_000, 16_000);
        let out = r.process(&[]);
        assert_eq!(out, Vec::<f32>::new());
    }

    #[test]
    fn f32_to_pcm16_clamps_above_one() {
        let out = f32_to_pcm16(&[1.5, 2.0, -1.5, -2.0]);
        assert_eq!(out, vec![i16::MAX, i16::MAX, -i16::MAX, -i16::MAX]);
    }

    #[test]
    fn f32_to_pcm16_scales_full_range() {
        let out = f32_to_pcm16(&[1.0, -1.0, 0.0, 0.5]);
        assert_eq!(out[0], i16::MAX);
        assert_eq!(out[1], -i16::MAX); // symmetric clamp, not i16::MIN
        assert_eq!(out[2], 0);
        assert!((out[3] - (i16::MAX / 2)).abs() <= 1);
    }

    #[test]
    fn f32_to_pcm16_handles_nan_and_inf() {
        let out = f32_to_pcm16(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY]);
        assert_eq!(out, vec![0, i16::MAX, i16::MIN]);
    }
}
