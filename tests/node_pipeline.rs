use pole_protocol_draft::{
    cid_from_hash, stable_hash32, BatchBuilder, NodePipelineError, SteamCurrentPlayersSample,
};

fn fixed32(byte: u8) -> [u8; 32] {
    [byte; 32]
}

#[test]
fn steam_sample_becomes_observation_record() {
    let sample = SteamCurrentPlayersSample::steam_current_players(
        730,
        777_777,
        1_700_000_000_000,
        "{\"response\":{\"player_count\":777777}}",
    );

    let observation = sample
        .into_observation(3, 9, fixed32(1), vec![1, 2, 3])
        .unwrap();

    assert_eq!(observation.epoch_id, 3);
    assert_eq!(observation.slot_id, 9);
    assert_eq!(observation.app_id, 730);
    assert_eq!(observation.observed_players, 777_777);
    assert_eq!(observation.collector_id, fixed32(1));
    assert!(observation
        .raw_body_cid
        .starts_with("cid://steam-observation/"));
}

#[test]
fn batch_builder_finalizes_deterministically() {
    let collector = fixed32(2);
    let mut builder = BatchBuilder::new(5, collector);

    let obs_a = SteamCurrentPlayersSample::steam_current_players(570, 111_111, 10, "a")
        .into_observation(5, 1, collector, vec![9])
        .unwrap();
    let obs_b = SteamCurrentPlayersSample::steam_current_players(730, 222_222, 20, "b")
        .into_observation(5, 2, collector, vec![8])
        .unwrap();

    builder.push(obs_b.clone()).unwrap();
    builder.push(obs_a.clone()).unwrap();

    let assembled = builder.finalize(42).unwrap();

    assert_eq!(assembled.batch_commit.epoch_id, 5);
    assert_eq!(assembled.batch_commit.collector_id, collector);
    assert_eq!(assembled.batch_commit.slot_start, 1);
    assert_eq!(assembled.batch_commit.slot_end, 2);
    assert_eq!(assembled.batch_commit.obs_count, 2);
    assert_eq!(assembled.batch_commit.submitted_at_height, 42);
    assert_eq!(assembled.batch_commit.payload_cid, assembled.payload_cid);
    assert_eq!(assembled.observations[0], obs_a);
    assert_eq!(assembled.observations[1], obs_b);
    assert!(assembled.payload_cid.starts_with("cid://batch-payload/"));
}

#[test]
fn batch_builder_rejects_mismatched_epoch_or_collector() {
    let collector = fixed32(3);
    let mut builder = BatchBuilder::new(7, collector);

    let wrong_epoch = SteamCurrentPlayersSample::steam_current_players(10, 1, 1, "epoch")
        .into_observation(8, 1, collector, vec![1])
        .unwrap();
    let err = builder.push(wrong_epoch).unwrap_err();
    assert!(matches!(
        err,
        NodePipelineError::MismatchedEpoch {
            expected: 7,
            actual: 8
        }
    ));

    let wrong_collector = SteamCurrentPlayersSample::steam_current_players(20, 2, 2, "collector")
        .into_observation(7, 1, fixed32(4), vec![1])
        .unwrap();
    let err = builder.push(wrong_collector).unwrap_err();
    assert!(matches!(
        err,
        NodePipelineError::MismatchedCollector { expected, actual }
        if expected == collector && actual == fixed32(4)
    ));
}

#[test]
fn helpers_produce_stable_hash_and_cid() {
    let hash = stable_hash32(b"pole");
    let cid = cid_from_hash(hash, "demo");

    assert_eq!(hash, stable_hash32(b"pole"));
    assert_ne!(hash, stable_hash32(b"pole2"));
    assert!(cid.starts_with("cid://demo/"));
}
