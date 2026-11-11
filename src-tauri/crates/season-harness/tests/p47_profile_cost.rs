//! Cost of one world-wide club strategy read (`strategy_reports`), mid-career. `#[ignore]`d.
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::env;

struct Timer {
    samples: Vec<f64>,
}

impl SeasonObserver for Timer {
    fn on_day_end(&mut self, game: &Game, season: u32, day: u32) {
        if season == 2 && day.is_multiple_of(30) {
            let started = std::time::Instant::now();
            for _ in 0..20 {
                std::hint::black_box(ofm_core::transfers::strategy_reports(game));
            }
            self.samples
                .push(started.elapsed().as_secs_f64() * 1e3 / 20.0);
        }
    }
}

#[test]
#[ignore = "measurement study"]
fn p47_profile_cost() {
    let mut game = env::world("P47_WORLD", 1).build().expect("world builds");
    let mut timer = Timer {
        samples: Vec::new(),
    };
    run_seasons(&mut game, RunOptions::seasons(3), &mut timer).expect("plays");
    eprintln!("strategy_reports ms per call: {:?}", timer.samples);
}
