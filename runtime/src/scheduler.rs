// EVERY block polling scheduler.
//
// DESIGN
//   Each EVERY block in a Ferrum program becomes a SchedulerSlot
//   at program startup. The main loop calls Scheduler::tick()
//   on every iteration, passing how many milliseconds elapsed
//   since the last tick. Each slot accumulates elapsed time and
//   fires its callback when the accumulated time meets its period.
//
// WHY NOT INTERRUPTS
//   Interrupt-based scheduling requires platform-specific setup
//   (NVIC, timer peripheral configuration) that varies significantly
//   between boards. The polling model works identically on every
//   supported board with zero platform-specific scheduler code.
//   For the educational context this language targets, polling
//   accuracy (±LOOP_TICK_MS) is entirely sufficient.
//
// USAGE IN GENERATED CODE
//   The emitter generates static mut counters and const periods
//   (see EVERY counter emission in rust_emit.rs). The Scheduler
//   type here is available for programs that want to use it
//   directly rather than the raw counter pattern.
//
// GENERATED PATTERN (already in rust_emit.rs, shown for reference):
//
//   static mut EVERY_0_COUNTER: u64 = 0;
//   const  EVERY_0_PERIOD_MS:   u64 = 1000;
//
//   // in run_loop():
//   unsafe { EVERY_0_COUNTER += LOOP_TICK_MS; }
//   if unsafe { EVERY_0_COUNTER } >= EVERY_0_PERIOD_MS {
//       unsafe { EVERY_0_COUNTER = 0; }
//       // body
//   }

/// A single scheduled slot — period in ms, accumulated elapsed time.
#[derive(Copy, Clone)]
pub struct SchedulerSlot {
    period_ms:  u64,
    elapsed_ms: u64,
}

impl SchedulerSlot {
    pub const fn new(period_ms: u64) -> Self {
        SchedulerSlot { period_ms, elapsed_ms: 0 }
    }

    /// Advance the counter by `delta_ms` milliseconds.
    /// Returns true if the slot should fire this tick.
    pub fn tick(&mut self, delta_ms: u64) -> bool {
        self.elapsed_ms += delta_ms;
        if self.elapsed_ms >= self.period_ms {
            self.elapsed_ms = 0;
            true
        } else {
            false
        }
    }

    /// Reset the counter without firing.
    pub fn reset(&mut self) { self.elapsed_ms = 0; }

    /// How many ms until the next fire.
    pub fn remaining_ms(&self) -> u64 {
        self.period_ms.saturating_sub(self.elapsed_ms)
    }
}

/// A fixed-capacity scheduler for up to N EVERY slots.
/// N is a const generic — chosen at program compile time.
pub struct Scheduler<const N: usize> {
    slots: [SchedulerSlot; N],
}

impl<const N: usize> Scheduler<N> {
    /// Construct from an array of period values in milliseconds.
    pub const fn new(periods: [u64; N]) -> Self {
        // Can't use array::map in const context on all stable Rust versions.
        // Build manually for up to N slots.
        // This is a compile-time operation — zero runtime overhead.
        let mut slots = [SchedulerSlot { period_ms: 0, elapsed_ms: 0 }; N];
        let mut i = 0;
        while i < N {
            slots[i].period_ms = periods[i];
            i += 1;
        }
        Scheduler { slots }
    }

    /// Advance all slots by `delta_ms`. Returns a bitmask of which slots fired.
    /// Bit 0 = slot 0, bit 1 = slot 1, etc.
    /// For programs with ≤ 64 EVERY blocks (more than enough).
    pub fn tick(&mut self, delta_ms: u64) -> u64 {
        let mut fired: u64 = 0;
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.tick(delta_ms) {
                fired |= 1 << i;
            }
        }
        fired
    }

    /// Check if slot `i` fired in the most recent tick result.
    pub fn did_fire(fired: u64, slot: usize) -> bool {
        (fired >> slot) & 1 == 1
    }
}

#[cfg(test)]
mod scheduler_tests {
    use super::*;

    #[test]
    fn slot_fires_exactly_at_period() {
        let mut slot = SchedulerSlot::new(1000);
        assert!(!slot.tick(500));
        assert!(!slot.tick(499));
        assert!(slot.tick(1));   // exactly 1000ms accumulated
    }

    #[test]
    fn slot_resets_after_fire() {
        let mut slot = SchedulerSlot::new(100);
        slot.tick(100);          // fires
        assert!(!slot.tick(99)); // should not fire again yet
        assert!(slot.tick(1));   // fires again at 100ms
    }

    #[test]
    fn slot_remaining_decreases() {
        let mut slot = SchedulerSlot::new(500);
        slot.tick(200);
        assert_eq!(slot.remaining_ms(), 300);
        slot.tick(300);          // fires
        assert_eq!(slot.remaining_ms(), 500); // reset
    }

    #[test]
    fn slot_overshooting_period_fires_once() {
        // If a tick delta exceeds the period (e.g. system was busy),
        // the slot fires once and resets — it does not fire multiple times.
        let mut slot = SchedulerSlot::new(100);
        assert!(slot.tick(250)); // overshoot — fires once
        // Elapsed is now 0 (reset), not 150
        assert_eq!(slot.elapsed_ms, 0);
    }

    #[test]
    fn scheduler_two_slots_independent() {
        let mut sched: Scheduler<2> = Scheduler::new([100, 500]);
        let fired = sched.tick(100);
        assert!(Scheduler::<2>::did_fire(fired, 0)); // 100ms slot fires
        assert!(!Scheduler::<2>::did_fire(fired, 1)); // 500ms slot does not
    }

    #[test]
    fn scheduler_both_fire_at_lcm() {
        let mut sched: Scheduler<2> = Scheduler::new([100, 200]);
        // Tick 200ms total in two 100ms steps
        sched.tick(100);
        let fired = sched.tick(100); // both at 200ms
        assert!(Scheduler::<2>::did_fire(fired, 0));
        assert!(Scheduler::<2>::did_fire(fired, 1));
    }

    #[test]
    fn scheduler_const_construction() {
        // Verify it can be constructed in a const context
        const S: Scheduler<3> = Scheduler::new([50, 100, 200]);
        let _ = S;
    }
}