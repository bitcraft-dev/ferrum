// Button debounce helper.
//
// DESIGN
//   The Ferrum spec supports DEBOUNCE_MS in CONFIG. Generated code
//   reads button state with is_high()/is_low(). Without debounce,
//   mechanical buttons produce multiple transitions per press.
//
//   This module provides a simple counter-based debouncer that
//   is board-independent and requires no timer peripheral — it
//   uses the loop tick counter that already exists for EVERY blocks.
//
// USAGE
//   Generated code for an INPUT device with debounce enabled wraps
//   each button read in a Debouncer::sample() call.
//
//   Generated pattern (when DEBOUNCE_MS is set in CONFIG):
//
//     static mut BTN_DEBOUNCER: Debouncer = Debouncer::new(50);
//
//     // in run_loop():
//     let mode_btn_raw = mode_btn.input.is_low().unwrap();
//     let mode_btn     = unsafe { BTN_DEBOUNCER.sample(mode_btn_raw, LOOP_TICK_MS) };

/// Counter-based debouncer.
/// Requires the input to be stable for `threshold_ms` before a
/// state change is reported.
pub struct Debouncer {
    threshold_ms: u32,
    counter_ms:   u32,
    stable_state: bool,
    /// `None` until the first sample arrives. Using an `Option` (rather
    /// than defaulting to `false`) keeps the very first reading from
    /// being misread as a "changed" transition when it happens to be
    /// `true`, which would otherwise throw away that sample's own
    /// `delta_ms` and require a full extra threshold's worth of ticks.
    last_raw:     Option<bool>,
}

impl Debouncer {
    pub const fn new(threshold_ms: u32) -> Self {
        Debouncer {
            threshold_ms,
            counter_ms:   0,
            stable_state: false,
            last_raw:     None,
        }
    }

    /// Feed a new raw sample.
    /// `delta_ms` is time elapsed since last call.
    /// Returns the debounced (stable) state.
    pub fn sample(&mut self, raw: bool, delta_ms: u32) -> bool {
        if self.last_raw == Some(raw) {
            // Signal is stable — accumulate time
            self.counter_ms = self.counter_ms.saturating_add(delta_ms);
        } else {
            // Signal changed (or this is the first-ever sample) —
            // restart timing from this sample's own delta, not zero.
            self.counter_ms = delta_ms;
            self.last_raw   = Some(raw);
        }
        if self.counter_ms >= self.threshold_ms {
            self.stable_state = raw;
        }
        self.stable_state
    }

    /// Force the stable state without going through debounce.
    /// Used to set the initial state at program start.
    pub fn force(&mut self, state: bool) {
        self.stable_state = state;
        self.last_raw     = Some(state);
        self.counter_ms   = self.threshold_ms; // already stable
    }
}

#[cfg(test)]
mod debounce_tests {
    use super::*;

    #[test]
    fn state_changes_after_threshold() {
        let mut d = Debouncer::new(50);
        // Not yet stable — should return initial false
        assert!(!d.sample(true, 20));
        assert!(!d.sample(true, 20));
        // Total 40ms — not yet at threshold
        assert!(!d.sample(true, 9));
        // 49ms — still not there
        assert!(d.sample(true, 1));   // 50ms — state accepted
    }

    #[test]
    fn bounce_resets_counter() {
        let mut d = Debouncer::new(50);
        d.sample(true, 40); // 40ms of high
        d.sample(false, 5); // bounce back to low — counter resets
        d.sample(true, 45); // back to high, 45ms — not enough
        assert!(!d.sample(true, 4)); // 49ms total since last bounce
        assert!(d.sample(true, 1));  // 50ms — accepted
    }

    #[test]
    fn stable_low_stays_low() {
        let mut d = Debouncer::new(50);
        assert!(!d.sample(false, 100)); // stays low
        assert!(!d.sample(false, 100));
    }

    #[test]
    fn force_sets_initial_state() {
        let mut d = Debouncer::new(50);
        d.force(true);
        // Even a single sample with low shouldn't immediately change state
        assert!(d.sample(true, 1));
    }
}