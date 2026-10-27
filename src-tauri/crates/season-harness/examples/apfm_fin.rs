//! Day-one finance status of every club in the FM24 world, as the board would read it.
use season_harness::fm24::{build_fm24_world, world_path_from_env};

fn main() {
    let path = world_path_from_env().expect("APFM_WORLD_JSON");
    let (game, _) = build_fm24_world(&path, 7).expect("world");
    let mut by_status = std::collections::BTreeMap::<String, u32>::new();
    let mut rows = Vec::new();
    for t in &game.teams {
        let Some(s) = ofm_core::finances::team_finance_snapshot(&game, &t.id) else {
            continue;
        };
        *by_status
            .entry(format!("{:?}", s.overall_status))
            .or_default() += 1;
        rows.push((
            s.wage_budget_usage_percent,
            t.name.clone(),
            s.weekly_wage_spend,
            s.weekly_wage_budget,
            s.weekly_recurring_income,
            t.finance,
            format!("{:?}", s.wage_budget_status),
            format!("{:?}", s.runway_status),
        ));
    }
    let today = game.clock.current_date.format("%Y-%m-%d").to_string();
    let attached: Vec<_> = game
        .players
        .iter()
        .filter(|p| p.team_id.is_some())
        .collect();
    let future = attached
        .iter()
        .filter(|p| p.contract_start().is_some_and(|s| s > today.as_str()))
        .count();
    let expired = attached
        .iter()
        .filter(|p| p.contract_end().is_some_and(|e| e < today.as_str()))
        .count();
    let ends_first_month = attached
        .iter()
        .filter(|p| {
            p.contract_end()
                .is_some_and(|e| e >= today.as_str() && e <= "2026-07-31")
        })
        .count();
    println!(
        "date {today}: attached {} future-start contracts {future}, expired {expired}, ending within a month {ends_first_month}",
        attached.len()
    );
    println!("{by_status:?}");
    rows.sort_by_key(|r| std::cmp::Reverse(r.0));
    for r in rows.iter().take(12) {
        println!("{:?}", r);
    }
    for r in rows.iter().filter(|r| {
        [
            "FC Annecy",
            "Paris Saint-Germain",
            "RC Lens",
            "AFC Bournemouth",
        ]
        .contains(&r.1.as_str())
    }) {
        println!("{:?}", r);
    }
    let over: Vec<_> = rows.iter().filter(|r| r.0 > 100).count().to_string().into();
    println!("over 100% of wage budget: {over:?}");
}
