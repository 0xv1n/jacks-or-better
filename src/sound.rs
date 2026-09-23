use std::time::Duration;

use rodio::source::SineWave;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};

/// Short synthesized tones. Silent when no audio device can be opened.
pub struct Sound {
    sink: Option<MixerDeviceSink>,
}

impl Sound {
    pub fn new() -> Sound {
        let sink = DeviceSinkBuilder::open_default_sink().ok().map(|mut s| {
            s.log_on_drop(false);
            s
        });
        Sound { sink }
    }

    /// A plucked sine: full `gain` at the start, decaying to silence over `secs`.
    fn tone(&self, freq: f32, secs: f32, gain: f32, delay: f64) {
        if let Some(sink) = &self.sink {
            let len = Duration::from_secs_f32(secs);
            sink.mixer().add(
                SineWave::new(freq)
                    .take_duration(len)
                    .linear_gain_ramp(len, gain, 0.0, true)
                    .delay(Duration::from_secs_f64(delay.max(0.0))),
            );
        }
    }

    /// A card turning face up.
    pub fn flip(&self, delay: f64) {
        self.tone(1320.0, 0.035, 0.07, delay);
    }

    pub fn hold(&self, held: bool) {
        self.tone(if held { 880.0 } else { 660.0 }, 0.05, 0.12, 0.0);
    }

    /// A rising arpeggio, longer for bigger wins (`multiple` = payout per coin bet).
    pub fn win(&self, multiple: u32, delay: f64) {
        const NOTES: [f32; 6] = [523.25, 659.25, 783.99, 1046.5, 1318.5, 1568.0];
        let count = match multiple {
            0 => return,
            1 => 2,
            2..=3 => 3,
            4..=9 => 4,
            10..=49 => 5,
            _ => 6,
        };
        for (i, &freq) in NOTES.iter().take(count).enumerate() {
            self.tone(freq, 0.22, 0.14, delay + i as f64 * 0.085);
        }
    }
}
