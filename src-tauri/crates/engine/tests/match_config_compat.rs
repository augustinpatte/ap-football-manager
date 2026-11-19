//! A match configuration written before the I5 group-depth rule must keep loading, and must keep
//! meaning the engine it was calibrated on.

use engine::MatchConfig;

/// Given a serialized `MatchConfig` written before `group_depth_weight` existed (the field absent)
/// When it is deserialized
/// Then it loads, and the new weight reads 0, which is the plain mean of the old engine.
#[test]
fn a_configuration_written_before_group_depth_loads_with_weight_zero() {
    let mut value = serde_json::to_value(MatchConfig::default()).expect("the default serializes");
    let fields = value.as_object_mut().expect("a config is an object");
    assert!(
        fields.remove("group_depth_weight").is_some(),
        "the field is serialized today, so removing it models an old save"
    );

    let old: MatchConfig = serde_json::from_value(value).expect("an old configuration loads");

    assert_eq!(old.group_depth_weight, 0.0);
    // Everything an old configuration did carry is read as it was written.
    assert_eq!(old.home_advantage, MatchConfig::default().home_advantage);
}

/// Given a configuration written today
/// When it is round-tripped through JSON
/// Then the shipped weight (1.0) survives, so a new save plays the depth-aware rule it was made with.
#[test]
fn a_current_configuration_round_trips_the_shipped_group_depth_weight() {
    let json = serde_json::to_string(&MatchConfig::default()).expect("serializes");
    let back: MatchConfig = serde_json::from_str(&json).expect("loads");
    assert_eq!(back.group_depth_weight, 1.0);
}
