//! # Timings
//!
//! Records how long each part of `render()` takes, and prints the averages every few frames.

use std::time::Duration;

/// Time spent in each part of render(), added up until the next report
#[derive(Default)]
pub(super) struct RenderTimings {
    pub(super) frames : u32,          // frames added since the last report
    pub(super) resize : Duration,     // recreating render targets
    pub(super) record : Duration,     // recording the command buffer
    pub(super) submit : Duration,     // submitting to the queue
    pub(super) wait : Duration,       // waiting for the GPU to finish
    pub(super) total : Duration,      // whole render() call
}

impl RenderTimings {
    const FRAMES_PER_REPORT : u32 = 60;

    /// Print the average per frame and start counting again, once enough frames have been added
    pub(super) fn report_if_due(&mut self) {
        if self.frames < Self::FRAMES_PER_REPORT { return; }

        let average_ms = |time : Duration| time.as_secs_f64() * 1000.0 / self.frames as f64;
        eprintln!(
            "[render avg over {} frames] resize {:.3} ms | record {:.3} ms | submit {:.3} ms | gpu wait {:.3} ms | total {:.3} ms",
            self.frames,
            average_ms(self.resize),
            average_ms(self.record),
            average_ms(self.submit),
            average_ms(self.wait),
            average_ms(self.total),
        );

        *self = Self::default();
    }
}
