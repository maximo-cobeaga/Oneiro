use std::path::Path;

/// Internal sample rate used for mixing and export.
pub const SR: u32 = 48_000;

/// Mono floating point audio.
#[derive(Clone, Debug)]
pub struct Audio {
    pub sr: u32,
    pub samples: Vec<f32>,
}

impl Audio {
    pub fn duration(&self) -> f64 {
        self.samples.len() as f64 / self.sr as f64
    }

    pub fn resampled(&self, target: u32) -> Audio {
        if self.sr == target || self.samples.is_empty() {
            return Audio { sr: target, samples: self.samples.clone() };
        }
        let ratio = self.sr as f64 / target as f64;
        let n = (self.samples.len() as f64 / ratio) as usize;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let p = i as f64 * ratio;
            let i0 = p.floor() as usize;
            let f = (p - i0 as f64) as f32;
            let a = self.samples[i0.min(self.samples.len() - 1)];
            let b = self.samples[(i0 + 1).min(self.samples.len() - 1)];
            out.push(a + (b - a) * f);
        }
        Audio { sr: target, samples: out }
    }
}

pub fn read_wav(path: &Path) -> Result<Audio, String> {
    let mut reader = hound::WavReader::open(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        }
    };
    let mut mono = Vec::with_capacity(interleaved.len() / ch);
    for frame in interleaved.chunks(ch) {
        mono.push(frame.iter().sum::<f32>() / frame.len() as f32);
    }
    Ok(Audio { sr: spec.sample_rate, samples: mono })
}

pub fn write_wav(path: &Path, samples: &[f32], sr: u32) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sr,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| format!("{}: {}", path.display(), e))?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        w.write_sample(v).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())
}
