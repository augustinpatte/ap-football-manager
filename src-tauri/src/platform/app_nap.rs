//! Keeps macOS App Nap off while the game is simulating.
//!
//! App Nap throttles a process that has no visible, frontmost window: after roughly half a minute
//! the scheduler drops its priority and CPU-bound work runs eight to eleven times slower. The
//! day loop is exactly that kind of work, and it is driven by clients that have no window to keep
//! it awake — the MCP server runs with `--no-gui`, and a player can hide the window while a long
//! skip runs. Mission 11 measured 30 ms per game day before the throttle and 220 to 380 ms after
//! it, from the very first day of a process that had sat idle; with the process opted out the
//! figure stayed at 28 to 43 ms.
//!
//! An activity is the documented opt-out. Ending one (`endActivity`) is `unsafe` in the bindings
//! and this workspace forbids `unsafe`, so the activity is taken once and kept for the life of the
//! process rather than scoped to a command. `UserInitiatedAllowingIdleSystemSleep` keeps the
//! process out of App Nap without stopping the Mac from sleeping when it is idle.
use std::sync::Once;

use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

static BEGIN: Once = Once::new();

pub fn keep_simulation_awake() {
    BEGIN.call_once(|| {
        let token = NSProcessInfo::processInfo().beginActivityWithOptions_reason(
            NSActivityOptions::UserInitiatedAllowingIdleSystemSleep,
            &NSString::from_str("AP Football Manager is simulating the game world"),
        );
        // Dropping the token would release it; the activity is meant to outlive every caller.
        std::mem::forget(token);
    });
}
