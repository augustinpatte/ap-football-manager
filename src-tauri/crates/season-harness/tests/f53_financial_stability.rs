//! Run F5.3: the poorest clubs of the shipped world do not slide into debt.
//!
//! The 80-club gate world (England, Portugal, Argentina) hid the shipped world's economy: its debt
//! sits in clubs whose revenue is a quarter of a big club's and does not grow, while the wages they
//! pay used to ratchet (a renewal at +37 %, a recruit paid like the first team, an over-budget club
//! allowed a further 3 % of its budget at every one of a dozen decisions a year). This plays the four
//! poorest nations (Wales, Northern Ireland, Ireland, Sweden: 80 clubs, `WorldSpec::lower_nations`)
//! through ten seasons and asks the books. Before F5.3 the same world, same seed, ended season ten
//! with 27 of 79 clubs in debt and 73 over their wage budgets. The full 440-club study and the thirty
//! seasons are `tests/f53_finance_study.rs` (measurement, `#[ignore]`d); this is the part that runs
//! with the suite.

use ofm_core::finances::calc_wages;
use ofm_core::game::Game;
use season_harness::driver::{RunOptions, SeasonObserver, run_seasons};
use season_harness::worlds::WorldSpec;

struct Quiet;

impl SeasonObserver for Quiet {}

struct Books {
    clubs: usize,
    in_debt: usize,
    over_budget: usize,
}

/// The clubs the manager does not run: the harness's manager makes no decisions, so its club's books
/// say nothing about the AI's.
fn books_of_the_ai_clubs(game: &Game) -> Books {
    let user = game.manager.team_id.as_deref();
    let clubs: Vec<_> = game
        .teams
        .iter()
        .filter(|team| Some(team.id.as_str()) != user)
        .collect();
    Books {
        clubs: clubs.len(),
        in_debt: clubs.iter().filter(|team| team.finance < 0).count(),
        over_budget: clubs
            .iter()
            .filter(|team| {
                team.wage_budget > 0 && calc_wages(game, &team.id) * 10 > team.wage_budget * 11
            })
            .count(),
    }
}

/// Given the four poorest nations of the shipped world, two different worlds
/// When ten seasons are played
/// Then few clubs are in debt and few pay over 110 % of their wage budget: the wage bills follow
/// what the clubs earn instead of ratcheting past it.
#[test]
fn the_poorest_clubs_do_not_slide_into_debt_over_ten_seasons() {
    for seed in [1u64, 2] {
        let mut game = WorldSpec::lower_nations(seed)
            .build()
            .expect("the world builds");
        run_seasons(&mut game, RunOptions::seasons(10), &mut Quiet).expect("ten seasons play");

        let books = books_of_the_ai_clubs(&game);
        assert!(books.clubs >= 70, "{} clubs", books.clubs);
        assert!(
            books.in_debt * 100 <= books.clubs * 10,
            "seed {seed}: {} of {} clubs in debt after ten seasons",
            books.in_debt,
            books.clubs
        );
        assert!(
            books.over_budget * 100 <= books.clubs * 15,
            "seed {seed}: {} of {} clubs pay over 110 % of their wage budget",
            books.over_budget,
            books.clubs
        );
    }
}
