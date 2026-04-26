use pole_protocol_draft::{
    execute_block, BatchCommit, Block, Capability, Challenge, ChallengeEvidenceRef,
    ChallengeResolution, ChallengeResponseTx, ChallengeState, ClaimRewardTx, CommitEpochTx,
    EpochCommit, FeeParams, GovernanceParams, MerkleCommitment, NodeRegistry, NodeStatus,
    OpenChallengeTx, ProposeProtocolParamsUpdateTx, ProtocolParams, ProtocolState, ProtocolStore,
    RewardParams, RewardRecord, SlashingParams, StakeTx, SubmitBatchTx, Transaction, TransferTx,
    TransitionError, UnbondTx, VoteChoice, VoteTx,
};

fn fixed32(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn test_params() -> ProtocolParams {
    ProtocolParams {
        slot_seconds: 300,
        epoch_slots: 12,
        committee_size: 21,
        unbonding_blocks: 5,
        min_verify_bond: 100,
        min_propose_bond: 10_000,
        challenge_window_blocks: 20,
        max_emergency_brake_blocks: 100,
        min_retention_epochs: 2,
        fee: FeeParams {
            base_gas_price_nano: 100,
            max_gas_price_nano: 1_000,
            gas_adjustment_ppm: 1_150_000,
            congestion_threshold_ppm: 500_000,
            fee_burn_bps: 2_500,
        },
        rewards: RewardParams {
            reward_source_is_tokenomics: false,
            emission_year: 1,
            reward_block_secs: 3_600,
            initial_emission_rate_bps: 2_000,
            tail_emission_start_year: 4,
            tail_emission_rate_bps: 200,
            player_reward_allocation_bps: 8_000,
            service_reward_allocation_bps: 1_000,
            collect_reward_bps: 5_000,
            store_reward_bps: 2_500,
            verify_reward_bps: 1_500,
            propose_reward_bps: 1_000,
            configured_player_block_reward: 1_000,
            effective_player_block_reward: 1_000,
            target_network_weight_units: 150_000_000_000_000,
            reward_adjustment_cap_bps: 2_000,
            tier1_weight_ppm: 1_000_000,
            tier2_weight_min_ppm: 300_000,
            tier2_weight_max_ppm: 600_000,
            tier3_weight_min_ppm: 50_000,
            tier3_weight_max_ppm: 150_000,
            app_weight_overrides: Vec::new(),
            reward_burn_threshold: 10_000,
            reward_burn_bps: 1_000,
            governance_burn_bps: 100,
        },
        governance: GovernanceParams {
            params_update_bond: 10_000,
            params_update_quorum_bps: 2_500,
            params_update_approval_bps: 6_000,
            slow_params_update_bond: 20_000,
            slow_params_update_quorum_bps: 3_300,
            slow_params_update_approval_bps: 7_500,
        },
        slashing: SlashingParams {
            double_sign_bps: 5_000,
            offline_bps: 100,
            medium_deviation_bps: 500,
            severe_deviation_bps: 2_000,
        },
    }
}

fn active_node(
    node_id: [u8; 32],
    reward_address: [u8; 32],
    bond: u128,
    caps: Vec<Capability>,
) -> NodeRegistry {
    NodeRegistry {
        node_id,
        pubkey: vec![1, 2, 3],
        reward_address,
        bond,
        status: NodeStatus::Active,
        enabled_capabilities: caps,
        reputation: pole_protocol_draft::ReputationSnapshot {
            score_ppm: 1_000_000,
            successful_challenges: 0,
            failed_challenges: 0,
            challengeable_faults: 0,
            collection_successes: 0,
            collection_failures: 0,
            storage_proofs_passed: 0,
            storage_proofs_failed: 0,
            last_updated_epoch: 1,
        },
        joined_at_height: 1,
    }
}

fn merkle(byte: u8) -> MerkleCommitment {
    MerkleCommitment {
        root: fixed32(byte),
        leaf_count: 1,
    }
}

#[test]
fn rewards_root_matches_reward_records_fixture() {
    let params = test_params();
    let mut state = ProtocolState::new(params, 1, 1);

    let collector = fixed32(1);
    let proposer = fixed32(2);

    state.upsert_node(active_node(
        collector,
        fixed32(11),
        0,
        vec![Capability::Collect],
    ));
    state.upsert_node(active_node(
        proposer,
        fixed32(12),
        20_000,
        vec![Capability::Propose],
    ));

    // Two reward records, sorted by node_id to match the canonical root expectation.
    let mut rewards = vec![
        RewardRecord {
            epoch_id: 1,
            node_id: collector,
            player_reward: 0,
            collect_reward: 0,
            store_reward: 0,
            verify_reward: 0,
            propose_reward: 0,
            slash_debit: 0,
            net_reward: 111,
        },
        RewardRecord {
            epoch_id: 1,
            node_id: proposer,
            player_reward: 0,
            collect_reward: 0,
            store_reward: 0,
            verify_reward: 0,
            propose_reward: 0,
            slash_debit: 0,
            net_reward: 222,
        },
    ];
    rewards.sort_by_key(|record| record.node_id);
    let rewards_root = pole_protocol_draft::reward_record_root(&rewards).unwrap();

    let submit = SubmitBatchTx {
        batch_commit: BatchCommit {
            epoch_id: 1,
            collector_id: collector,
            slot_start: 1,
            slot_end: 1,
            batch: merkle(21),
            payload_cid: "cid://batch-1".to_string(),
            obs_count: 1,
            submitted_at_height: 0,
        },
        signature: vec![7],
    };
    let commit = CommitEpochTx {
        epoch_commit: EpochCommit {
            epoch_id: 1,
            accepted_batches: merkle(31),
            observations: merkle(32),
            aggregates: merkle(33),
            rewards: MerkleCommitment {
                root: rewards_root,
                leaf_count: rewards.len() as u32,
            },
            availability: merkle(35),
            randomness_seed: fixed32(36),
            proposer_id: proposer,
            challenge_open_height: 0,
            challenge_deadline_height: 15,
        },
        signature: vec![8],
    };

    execute_block(
        &mut state,
        Block {
            height: 5,
            transactions: vec![
                Transaction::SubmitBatch(submit),
                Transaction::CommitEpoch(commit),
            ],
        },
    )
    .unwrap();

    let stored = state.store.epoch_commit(&1).unwrap();
    assert_eq!(stored.rewards.root, rewards_root);
    assert_eq!(stored.rewards.leaf_count, 2);

    // Store the same reward records that the root commits to.
    for record in rewards {
        state.upsert_reward_record(record);
    }
    state.store.mark_epoch_finalized(1);
}

#[test]
fn protocol_flow_commit_challenge_finalize_claim_works() {
    let params = test_params();
    let mut state = ProtocolState::new(params, 1, 1);

    let collector = fixed32(1);
    let proposer = fixed32(2);
    let target = fixed32(3);
    let challenger = fixed32(4);
    let claimer = fixed32(12);

    state.upsert_node(active_node(
        collector,
        fixed32(11),
        0,
        vec![Capability::Collect],
    ));
    state.upsert_node(active_node(
        proposer,
        fixed32(12),
        20_000,
        vec![Capability::Propose, Capability::Verify],
    ));
    state.upsert_node(active_node(
        target,
        fixed32(13),
        5_000,
        vec![Capability::Verify],
    ));

    state.upsert_account(pole_protocol_draft::AccountState {
        address: challenger,
        balance: 2_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_account(pole_protocol_draft::AccountState {
        address: claimer,
        balance: 0,
        staked: 0,
        locked: 0,
        nonce: 0,
    });

    let submit = SubmitBatchTx {
        batch_commit: BatchCommit {
            epoch_id: 1,
            collector_id: collector,
            slot_start: 1,
            slot_end: 1,
            batch: merkle(21),
            payload_cid: "cid://batch-1".to_string(),
            obs_count: 1,
            submitted_at_height: 0,
        },
        signature: vec![7],
    };
    let commit = CommitEpochTx {
        epoch_commit: EpochCommit {
            epoch_id: 1,
            accepted_batches: merkle(31),
            observations: merkle(32),
            aggregates: merkle(33),
            rewards: merkle(34),
            availability: merkle(35),
            randomness_seed: fixed32(36),
            proposer_id: proposer,
            challenge_open_height: 0,
            challenge_deadline_height: 15,
        },
        signature: vec![8],
    };

    execute_block(
        &mut state,
        Block {
            height: 5,
            transactions: vec![
                Transaction::SubmitBatch(submit),
                Transaction::CommitEpoch(commit),
            ],
        },
    )
    .unwrap();

    let open = OpenChallengeTx {
        challenge: Challenge {
            challenge_id: fixed32(41),
            kind: pole_protocol_draft::ChallengeKind::BadAggregate,
            epoch_id: 1,
            target_node: Some(target),
            challenger,
            bond: 100,
            opened_at_height: 0,
            deadline_height: 14,
            state: ChallengeState::Open,
            evidence: ChallengeEvidenceRef {
                batch_root: Some(fixed32(21)),
                aggregate_root: Some(fixed32(33)),
                reward_root: None,
                payload_cid: Some("cid://batch-1".to_string()),
                merkle_proof: vec![fixed32(51)],
            },
        },
        signature: vec![9],
    };

    execute_block(
        &mut state,
        Block {
            height: 6,
            transactions: vec![Transaction::OpenChallenge(open)],
        },
    )
    .unwrap();

    state
        .resolve_challenge(
            fixed32(41),
            ChallengeResolution::BadAggregate {
                slash_amount: 500,
                challenger_reward: 200,
                corrected_aggregate_root: Some(fixed32(62)),
            },
        )
        .unwrap();

    state.upsert_reward_record(RewardRecord {
        epoch_id: 1,
        node_id: target,
        player_reward: 0,
        collect_reward: 0,
        store_reward: 0,
        verify_reward: 0,
        propose_reward: 0,
        slash_debit: 500,
        net_reward: 300,
    });

    state.upsert_reward_record(RewardRecord {
        epoch_id: 1,
        node_id: proposer,
        player_reward: 0,
        collect_reward: 0,
        store_reward: 0,
        verify_reward: 0,
        propose_reward: 300,
        slash_debit: 0,
        net_reward: 300,
    });

    state.height = 16;
    state.finalize_epoch(1).unwrap();

    let claim = ClaimRewardTx {
        claimer,
        epoch_id: 1,
        node_id: proposer,
        amount: 300,
        merkle_proof: vec![fixed32(61)],
        nonce: 0,
        signature: vec![10],
    };
    state.apply_claim_reward(claim).unwrap();

    assert_eq!(state.current_epoch, 2);
    assert_eq!(state.store.node(&target).unwrap().bond, 4_500);
    assert_eq!(state.store.account(&challenger).unwrap().balance, 2_200);
    assert_eq!(state.store.account(&challenger).unwrap().locked, 0);
    assert_eq!(state.store.account(&claimer).unwrap().balance, 300);
    assert!(state.store.is_epoch_finalized(1));
    let resolution = state.store.challenge_resolution(&fixed32(41)).unwrap();
    assert!(matches!(
        &resolution.details,
        pole_protocol_draft::ChallengeResolutionDetails::BadAggregate {
            corrected_aggregate_root: Some(root)
        } if *root == fixed32(62)
    ));
}

#[test]
fn transfer_and_proto_roundtrip_work() {
    let tx = TransferTx {
        from: fixed32(71),
        to: fixed32(72),
        amount: 123,
        fee: 7,
        nonce: 2,
        signature: vec![1, 2, 3],
    };
    let proto = pole_protocol_draft::ProtoTransferTx::from(tx.clone());
    let roundtrip = TransferTx::try_from(proto).unwrap();
    assert_eq!(tx, roundtrip);

    let stake = pole_protocol_draft::StakeTx {
        delegator: fixed32(73),
        operator: fixed32(74),
        amount: 999,
        nonce: 4,
        signature: vec![4],
    };
    let stake_roundtrip = pole_protocol_draft::StakeTx::try_from(
        pole_protocol_draft::ProtoStakeTx::from(stake.clone()),
    )
    .unwrap();
    assert_eq!(stake, stake_roundtrip);

    let unbond = UnbondTx {
        delegator: fixed32(77),
        operator: fixed32(78),
        amount: 222,
        nonce: 5,
        signature: vec![6],
    };
    let unbond_roundtrip =
        UnbondTx::try_from(pole_protocol_draft::ProtoUnbondTx::from(unbond.clone())).unwrap();
    assert_eq!(unbond, unbond_roundtrip);

    let vote = VoteTx {
        proposal_id: fixed32(75),
        voter: fixed32(76),
        choice: VoteChoice::Yes,
        voting_power: 456,
        nonce: 9,
        signature: vec![5],
    };
    let vote_roundtrip =
        VoteTx::try_from(pole_protocol_draft::ProtoVoteTx::from(vote.clone())).unwrap();
    assert_eq!(vote, vote_roundtrip);

    let response = ChallengeResponseTx {
        challenge_id: fixed32(79),
        responder: fixed32(80),
        response_payload_cid: Some("cid://response-roundtrip".into()),
        response_hash: Some(fixed32(81)),
        signature: vec![7],
    };
    let response_roundtrip = ChallengeResponseTx::try_from(
        pole_protocol_draft::ProtoChallengeResponseTx::from(response.clone()),
    )
    .unwrap();
    assert_eq!(response, response_roundtrip);
}

#[test]
fn executor_rejects_height_regression() {
    let params = test_params();
    let mut state = ProtocolState::new(params, 10, 1);
    let result = execute_block(
        &mut state,
        Block {
            height: 10,
            transactions: Vec::new(),
        },
    );

    assert!(matches!(
        result,
        Err(pole_protocol_draft::BlockExecutionError::HeightRegression { .. })
    ));
}

#[test]
fn protocol_params_update_tx_proto_roundtrip_works() {
    let tx = ProposeProtocolParamsUpdateTx {
        proposal_id: fixed32(77),
        proposer: fixed32(78),
        effective_epoch: 3,
        params: test_params(),
        nonce: 9,
        signature: vec![1, 2, 3],
    };

    let roundtrip = ProposeProtocolParamsUpdateTx::try_from(
        pole_protocol_draft::ProtoProposeProtocolParamsUpdateTx::from(tx.clone()),
    )
    .unwrap();
    assert_eq!(tx, roundtrip);
}

#[test]
fn stake_unbond_vote_and_challenge_response_flow_works() {
    let params = test_params();
    let mut state = ProtocolState::new(params, 1, 1);

    let delegator = fixed32(81);
    let operator = fixed32(82);
    let challenger = fixed32(83);

    state.upsert_account(pole_protocol_draft::AccountState {
        address: delegator,
        balance: 5_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_account(pole_protocol_draft::AccountState {
        address: challenger,
        balance: 1_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_node(active_node(
        operator,
        fixed32(84),
        10_000,
        vec![Capability::Verify, Capability::Propose],
    ));

    execute_block(
        &mut state,
        Block {
            height: 2,
            transactions: vec![Transaction::Stake(StakeTx {
                delegator,
                operator,
                amount: 500,
                nonce: 0,
                signature: vec![1],
            })],
        },
    )
    .unwrap();

    assert_eq!(state.store.account(&delegator).unwrap().balance, 4_500);
    assert_eq!(state.store.account(&delegator).unwrap().staked, 500);
    assert_eq!(state.store.node(&operator).unwrap().bond, 10_500);

    let proposal_id = fixed32(85);
    state
        .apply_vote(VoteTx {
            proposal_id,
            voter: delegator,
            choice: VoteChoice::No,
            voting_power: 250,
            nonce: 1,
            signature: vec![2],
        })
        .unwrap();

    state.upsert_node(active_node(
        fixed32(86),
        fixed32(87),
        0,
        vec![Capability::Collect],
    ));
    state
        .apply_commit_epoch(CommitEpochTx {
            epoch_commit: EpochCommit {
                epoch_id: 1,
                accepted_batches: merkle(91),
                observations: merkle(92),
                aggregates: merkle(93),
                rewards: merkle(94),
                availability: merkle(95),
                randomness_seed: fixed32(96),
                proposer_id: operator,
                challenge_open_height: 0,
                challenge_deadline_height: 20,
            },
            signature: vec![3],
        })
        .err();

    state
        .apply_submit_batch(SubmitBatchTx {
            batch_commit: BatchCommit {
                epoch_id: 1,
                collector_id: fixed32(86),
                slot_start: 1,
                slot_end: 1,
                batch: merkle(97),
                payload_cid: "cid://batch-2".into(),
                obs_count: 1,
                submitted_at_height: 0,
            },
            signature: vec![4],
        })
        .unwrap();

    state
        .apply_commit_epoch(CommitEpochTx {
            epoch_commit: EpochCommit {
                epoch_id: 1,
                accepted_batches: merkle(91),
                observations: merkle(92),
                aggregates: merkle(93),
                rewards: merkle(94),
                availability: merkle(95),
                randomness_seed: fixed32(96),
                proposer_id: operator,
                challenge_open_height: 0,
                challenge_deadline_height: 20,
            },
            signature: vec![5],
        })
        .unwrap();

    let challenge_id = fixed32(88);
    state
        .apply_open_challenge(OpenChallengeTx {
            challenge: Challenge {
                challenge_id,
                kind: pole_protocol_draft::ChallengeKind::BadStorage,
                epoch_id: 1,
                target_node: Some(operator),
                challenger,
                bond: 50,
                opened_at_height: 0,
                deadline_height: 15,
                state: ChallengeState::Open,
                evidence: ChallengeEvidenceRef {
                    batch_root: None,
                    aggregate_root: None,
                    reward_root: None,
                    payload_cid: Some("cid://batch-2".into()),
                    merkle_proof: vec![fixed32(98)],
                },
            },
            signature: vec![6],
        })
        .unwrap();

    state
        .apply_challenge_response(ChallengeResponseTx {
            challenge_id,
            responder: operator,
            response_payload_cid: Some("cid://response".into()),
            response_hash: Some(fixed32(99)),
            signature: vec![7],
        })
        .unwrap();

    execute_block(
        &mut state,
        Block {
            height: 3,
            transactions: vec![Transaction::Unbond(UnbondTx {
                delegator,
                operator,
                amount: 200,
                nonce: 2,
                signature: vec![8],
            })],
        },
    )
    .unwrap();

    assert_eq!(state.store.account(&delegator).unwrap().staked, 300);
    assert_eq!(state.store.account(&delegator).unwrap().locked, 200);
    assert_eq!(state.store.node(&operator).unwrap().bond, 10_300);
    assert!(state.store.challenge_response(&challenge_id).is_some());

    let effects = execute_block(
        &mut state,
        Block {
            height: 8,
            transactions: Vec::new(),
        },
    )
    .unwrap();

    assert!(effects.iter().any(|effect| matches!(
        effect,
        pole_protocol_draft::TransitionEffect::UnbondMatured { delegator: d, amount: 200, .. } if *d == delegator
    )));
    assert_eq!(state.store.account(&delegator).unwrap().balance, 4_700);
    assert_eq!(state.store.account(&delegator).unwrap().locked, 0);
    assert!(state.store.vote_record(&(proposal_id, delegator)).is_some());
}

#[test]
fn protocol_params_update_must_target_future_epoch() {
    let params = test_params();
    let mut state = ProtocolState::new(params.clone(), 1, 2);

    let err = state
        .schedule_protocol_params_update(2, params)
        .unwrap_err();
    assert!(matches!(
        err,
        TransitionError::ParamsUpdateMustTargetFutureEpoch {
            current_epoch: 2,
            effective_epoch: 2
        }
    ));
}

#[test]
fn protocol_params_update_activates_only_after_epoch_rolls_forward() {
    let params = test_params();
    let mut state = ProtocolState::new(params.clone(), 1, 1);

    let collector = fixed32(140);
    let proposer = fixed32(141);
    state.upsert_node(active_node(
        collector,
        fixed32(142),
        1_000,
        vec![Capability::Collect],
    ));
    state.upsert_node(active_node(
        proposer,
        fixed32(143),
        20_000,
        vec![Capability::Propose],
    ));

    state
        .apply_submit_batch(SubmitBatchTx {
            batch_commit: BatchCommit {
                epoch_id: 1,
                collector_id: collector,
                slot_start: 1,
                slot_end: 1,
                batch: merkle(150),
                payload_cid: "cid://sched-batch".into(),
                obs_count: 1,
                submitted_at_height: 0,
            },
            signature: vec![1],
        })
        .unwrap();

    state
        .apply_commit_epoch(CommitEpochTx {
            epoch_commit: EpochCommit {
                epoch_id: 1,
                accepted_batches: merkle(151),
                observations: merkle(152),
                aggregates: merkle(153),
                rewards: merkle(154),
                availability: merkle(155),
                randomness_seed: fixed32(156),
                proposer_id: proposer,
                challenge_open_height: 0,
                challenge_deadline_height: 20,
            },
            signature: vec![2],
        })
        .unwrap();

    let mut future_params = params.clone();
    future_params.rewards.emission_year = 3;
    future_params.rewards.effective_player_block_reward = 9_132;
    state
        .schedule_protocol_params_update(2, future_params)
        .unwrap();

    assert_eq!(state.params.rewards.emission_year, 1);
    state.height = 21;
    state.finalize_epoch(1).unwrap();
    assert_eq!(state.current_epoch, 2);
    assert_eq!(state.params.rewards.emission_year, 3);
    assert_eq!(state.params.rewards.effective_player_block_reward, 9_132);
}

#[test]
fn protocol_params_update_transaction_schedules_future_epoch_activation() {
    let params = test_params();
    let mut state = ProtocolState::new(params.clone(), 1, 1);
    let proposer = fixed32(160);
    let proposer_account = fixed32(161);
    let collector = fixed32(162);
    let voter = fixed32(173);

    state.upsert_account(pole_protocol_draft::AccountState {
        address: proposer_account,
        balance: 10_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_account(pole_protocol_draft::AccountState {
        address: voter,
        balance: 5_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_node(active_node(
        proposer,
        fixed32(163),
        20_000,
        vec![Capability::Propose],
    ));
    state.upsert_node(active_node(
        collector,
        fixed32(164),
        1_000,
        vec![Capability::Collect],
    ));

    state
        .apply_submit_batch(SubmitBatchTx {
            batch_commit: BatchCommit {
                epoch_id: 1,
                collector_id: collector,
                slot_start: 1,
                slot_end: 1,
                batch: merkle(165),
                payload_cid: "cid://gov-batch".into(),
                obs_count: 1,
                submitted_at_height: 0,
            },
            signature: vec![1],
        })
        .unwrap();

    state
        .apply_commit_epoch(CommitEpochTx {
            epoch_commit: EpochCommit {
                epoch_id: 1,
                accepted_batches: merkle(166),
                observations: merkle(167),
                aggregates: merkle(168),
                rewards: merkle(169),
                availability: merkle(170),
                randomness_seed: fixed32(171),
                proposer_id: proposer,
                challenge_open_height: 0,
                challenge_deadline_height: 20,
            },
            signature: vec![2],
        })
        .unwrap();

    let mut updated_params = params.clone();
    updated_params.rewards.emission_year = 4;
    updated_params.rewards.effective_player_block_reward = 9_132;

    let effects = execute_block(
        &mut state,
        Block {
            height: 2,
            transactions: vec![Transaction::ProposeProtocolParamsUpdate(
                ProposeProtocolParamsUpdateTx {
                    proposal_id: fixed32(172),
                    proposer: proposer_account,
                    effective_epoch: 2,
                    params: updated_params,
                    nonce: 0,
                    signature: vec![3],
                },
            )],
        },
    )
    .unwrap();

    assert!(effects.iter().any(|effect| matches!(
        effect,
        pole_protocol_draft::TransitionEffect::ProtocolParamsUpdateProposed {
            proposal_id,
            effective_epoch: 2,
            ..
        } if *proposal_id == fixed32(172)
    )));
    assert!(state.store.params_update_proposal(&fixed32(172)).is_some());
    assert_eq!(state.store.account(&proposer_account).unwrap().balance, 0);
    assert_eq!(
        state.store.account(&proposer_account).unwrap().locked,
        10_000
    );
    assert_eq!(state.params.rewards.emission_year, 1);
    assert!(state.store.scheduled_protocol_params(&2).is_none());

    state
        .apply_vote(VoteTx {
            proposal_id: fixed32(172),
            voter,
            choice: VoteChoice::Yes,
            voting_power: 4_000,
            nonce: 0,
            signature: vec![4],
        })
        .unwrap();

    assert!(state.store.scheduled_protocol_params(&2).is_some());
    assert_eq!(
        state.store.account(&proposer_account).unwrap().balance,
        10_000
    );
    assert_eq!(state.store.account(&proposer_account).unwrap().locked, 0);
    assert_eq!(
        state
            .store
            .params_update_proposal(&fixed32(172))
            .unwrap()
            .state,
        pole_protocol_draft::GovernanceProposalState::Scheduled
    );

    state.height = 21;
    state.finalize_epoch(1).unwrap();
    assert_eq!(state.current_epoch, 2);
    assert_eq!(state.params.rewards.emission_year, 4);
    assert_eq!(
        state
            .store
            .params_update_proposal(&fixed32(172))
            .unwrap()
            .state,
        pole_protocol_draft::GovernanceProposalState::Activated
    );
}

#[test]
fn protocol_params_update_requires_quorum_and_approval_threshold() {
    let params = test_params();
    let mut state = ProtocolState::new(params.clone(), 1, 1);
    let proposer = fixed32(180);
    let proposer_account = fixed32(181);
    let small_voter = fixed32(182);

    state.upsert_account(pole_protocol_draft::AccountState {
        address: proposer_account,
        balance: 10_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_account(pole_protocol_draft::AccountState {
        address: small_voter,
        balance: 1_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_node(active_node(
        proposer,
        fixed32(183),
        20_000,
        vec![Capability::Propose],
    ));
    state.upsert_node(active_node(
        fixed32(184),
        fixed32(185),
        1_000,
        vec![Capability::Collect],
    ));

    state
        .apply_submit_batch(SubmitBatchTx {
            batch_commit: BatchCommit {
                epoch_id: 1,
                collector_id: fixed32(184),
                slot_start: 1,
                slot_end: 1,
                batch: merkle(186),
                payload_cid: "cid://quorum-batch".into(),
                obs_count: 1,
                submitted_at_height: 0,
            },
            signature: vec![1],
        })
        .unwrap();

    state
        .apply_commit_epoch(CommitEpochTx {
            epoch_commit: EpochCommit {
                epoch_id: 1,
                accepted_batches: merkle(187),
                observations: merkle(188),
                aggregates: merkle(189),
                rewards: merkle(190),
                availability: merkle(191),
                randomness_seed: fixed32(192),
                proposer_id: proposer,
                challenge_open_height: 0,
                challenge_deadline_height: 20,
            },
            signature: vec![2],
        })
        .unwrap();

    let mut updated_params = params.clone();
    updated_params.rewards.emission_year = 5;
    state
        .apply_propose_protocol_params_update(ProposeProtocolParamsUpdateTx {
            proposal_id: fixed32(193),
            proposer: proposer_account,
            effective_epoch: 2,
            params: updated_params,
            nonce: 0,
            signature: vec![3],
        })
        .unwrap();

    state
        .apply_vote(VoteTx {
            proposal_id: fixed32(193),
            voter: small_voter,
            choice: VoteChoice::Yes,
            voting_power: 1_000,
            nonce: 0,
            signature: vec![4],
        })
        .unwrap();

    assert!(state.store.scheduled_protocol_params(&2).is_none());
}

#[test]
fn protocol_params_update_expires_and_burns_bond_when_epoch_passes_without_approval() {
    let params = test_params();
    let mut state = ProtocolState::new(params.clone(), 1, 1);
    let proposer = fixed32(190);
    let proposer_account = fixed32(191);
    let collector = fixed32(192);

    state.upsert_account(pole_protocol_draft::AccountState {
        address: proposer_account,
        balance: 10_000,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_node(active_node(
        proposer,
        fixed32(193),
        20_000,
        vec![Capability::Propose],
    ));
    state.upsert_node(active_node(
        collector,
        fixed32(194),
        1_000,
        vec![Capability::Collect],
    ));

    state
        .apply_submit_batch(SubmitBatchTx {
            batch_commit: BatchCommit {
                epoch_id: 1,
                collector_id: collector,
                slot_start: 1,
                slot_end: 1,
                batch: merkle(195),
                payload_cid: "cid://expire-batch".into(),
                obs_count: 1,
                submitted_at_height: 0,
            },
            signature: vec![1],
        })
        .unwrap();

    state
        .apply_commit_epoch(CommitEpochTx {
            epoch_commit: EpochCommit {
                epoch_id: 1,
                accepted_batches: merkle(196),
                observations: merkle(197),
                aggregates: merkle(198),
                rewards: merkle(199),
                availability: merkle(200),
                randomness_seed: fixed32(201),
                proposer_id: proposer,
                challenge_open_height: 0,
                challenge_deadline_height: 20,
            },
            signature: vec![2],
        })
        .unwrap();

    state
        .apply_propose_protocol_params_update(ProposeProtocolParamsUpdateTx {
            proposal_id: fixed32(202),
            proposer: proposer_account,
            effective_epoch: 2,
            params: params.clone(),
            nonce: 0,
            signature: vec![3],
        })
        .unwrap();

    assert_eq!(state.store.account(&proposer_account).unwrap().balance, 0);
    assert_eq!(
        state.store.account(&proposer_account).unwrap().locked,
        10_000
    );

    state.height = 21;
    state.finalize_epoch(1).unwrap();

    let proposal = state.store.params_update_proposal(&fixed32(202)).unwrap();
    assert_eq!(
        proposal.state,
        pole_protocol_draft::GovernanceProposalState::Expired
    );
    assert_eq!(state.store.account(&proposer_account).unwrap().balance, 0);
    assert_eq!(state.store.account(&proposer_account).unwrap().locked, 0);
}

#[test]
fn claim_reward_must_use_registered_reward_address() {
    let params = test_params();
    let mut state = ProtocolState::new(params, 20, 2);

    let node_id = fixed32(91);
    let reward_address = fixed32(92);
    let wrong_claimer = fixed32(93);

    state.upsert_node(active_node(
        node_id,
        reward_address,
        10_000,
        vec![Capability::Propose],
    ));
    state.upsert_account(pole_protocol_draft::AccountState {
        address: reward_address,
        balance: 0,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_account(pole_protocol_draft::AccountState {
        address: wrong_claimer,
        balance: 0,
        staked: 0,
        locked: 0,
        nonce: 0,
    });
    state.upsert_reward_record(RewardRecord {
        epoch_id: 1,
        node_id,
        player_reward: 0,
        collect_reward: 0,
        store_reward: 0,
        verify_reward: 0,
        propose_reward: 500,
        slash_debit: 0,
        net_reward: 500,
    });
    state.store.mark_epoch_finalized(1);

    let err = state
        .apply_claim_reward(ClaimRewardTx {
            claimer: wrong_claimer,
            epoch_id: 1,
            node_id,
            amount: 500,
            merkle_proof: vec![fixed32(94)],
            nonce: 0,
            signature: vec![1],
        })
        .unwrap_err();

    assert!(matches!(
        err,
        TransitionError::InvalidRewardClaimer {
            expected,
            actual
        } if expected == reward_address && actual == wrong_claimer
    ));
}

#[test]
fn vote_cannot_exceed_available_voting_power() {
    let params = test_params();
    let mut state = ProtocolState::new(params, 1, 1);
    let voter = fixed32(95);
    state.upsert_account(pole_protocol_draft::AccountState {
        address: voter,
        balance: 100,
        staked: 50,
        locked: 25,
        nonce: 0,
    });

    let err = state
        .apply_vote(VoteTx {
            proposal_id: fixed32(96),
            voter,
            choice: VoteChoice::Yes,
            voting_power: 500,
            nonce: 0,
            signature: vec![1],
        })
        .unwrap_err();

    assert!(matches!(
        err,
        TransitionError::InsufficientVotingPower {
            address,
            required,
            available
        } if address == voter && required == 500 && available == 175
    ));
}

#[test]
fn wrong_challenge_response_does_not_remove_open_challenge() {
    let params = test_params();
    let mut state = ProtocolState::new(params, 1, 1);

    let proposer = fixed32(97);
    let collector = fixed32(98);
    let target = fixed32(99);
    let wrong_responder = fixed32(100);
    let challenger = fixed32(101);

    state.upsert_node(active_node(
        proposer,
        fixed32(102),
        10_000,
        vec![Capability::Propose],
    ));
    state.upsert_node(active_node(
        collector,
        fixed32(103),
        0,
        vec![Capability::Collect],
    ));
    state.upsert_node(active_node(
        target,
        fixed32(104),
        1_000,
        vec![Capability::Verify],
    ));
    state.upsert_node(active_node(
        wrong_responder,
        fixed32(105),
        1_000,
        vec![Capability::Verify],
    ));
    state.upsert_account(pole_protocol_draft::AccountState {
        address: challenger,
        balance: 500,
        staked: 0,
        locked: 0,
        nonce: 0,
    });

    state
        .apply_submit_batch(SubmitBatchTx {
            batch_commit: BatchCommit {
                epoch_id: 1,
                collector_id: collector,
                slot_start: 1,
                slot_end: 1,
                batch: merkle(106),
                payload_cid: "cid://batch-3".into(),
                obs_count: 1,
                submitted_at_height: 0,
            },
            signature: vec![1],
        })
        .unwrap();
    state
        .apply_commit_epoch(CommitEpochTx {
            epoch_commit: EpochCommit {
                epoch_id: 1,
                accepted_batches: merkle(107),
                observations: merkle(108),
                aggregates: merkle(109),
                rewards: merkle(110),
                availability: merkle(111),
                randomness_seed: fixed32(112),
                proposer_id: proposer,
                challenge_open_height: 0,
                challenge_deadline_height: 20,
            },
            signature: vec![2],
        })
        .unwrap();

    let challenge_id = fixed32(113);
    state
        .apply_open_challenge(OpenChallengeTx {
            challenge: Challenge {
                challenge_id,
                kind: pole_protocol_draft::ChallengeKind::BadStorage,
                epoch_id: 1,
                target_node: Some(target),
                challenger,
                bond: 50,
                opened_at_height: 0,
                deadline_height: 15,
                state: ChallengeState::Open,
                evidence: ChallengeEvidenceRef {
                    batch_root: None,
                    aggregate_root: None,
                    reward_root: None,
                    payload_cid: Some("cid://batch-3".into()),
                    merkle_proof: vec![fixed32(114)],
                },
            },
            signature: vec![3],
        })
        .unwrap();

    let err = state
        .apply_challenge_response(ChallengeResponseTx {
            challenge_id,
            responder: wrong_responder,
            response_payload_cid: Some("cid://wrong".into()),
            response_hash: Some(fixed32(115)),
            signature: vec![4],
        })
        .unwrap_err();

    assert!(matches!(
        err,
        TransitionError::InvalidChallengeResponder {
            challenge_id: cid,
            expected,
            actual
        } if cid == challenge_id && expected == target && actual == wrong_responder
    ));
    assert!(state.store.open_challenge(&challenge_id).is_some());
}

#[test]
fn challenge_response_cannot_arrive_after_deadline() {
    let params = test_params();
    let mut state = ProtocolState::new(params, 1, 1);

    let proposer = fixed32(116);
    let collector = fixed32(117);
    let target = fixed32(118);
    let challenger = fixed32(119);

    state.upsert_node(active_node(
        proposer,
        fixed32(120),
        10_000,
        vec![Capability::Propose],
    ));
    state.upsert_node(active_node(
        collector,
        fixed32(121),
        0,
        vec![Capability::Collect],
    ));
    state.upsert_node(active_node(
        target,
        fixed32(122),
        1_000,
        vec![Capability::Verify],
    ));
    state.upsert_account(pole_protocol_draft::AccountState {
        address: challenger,
        balance: 500,
        staked: 0,
        locked: 0,
        nonce: 0,
    });

    state
        .apply_submit_batch(SubmitBatchTx {
            batch_commit: BatchCommit {
                epoch_id: 1,
                collector_id: collector,
                slot_start: 1,
                slot_end: 1,
                batch: merkle(123),
                payload_cid: "cid://batch-4".into(),
                obs_count: 1,
                submitted_at_height: 0,
            },
            signature: vec![1],
        })
        .unwrap();
    state
        .apply_commit_epoch(CommitEpochTx {
            epoch_commit: EpochCommit {
                epoch_id: 1,
                accepted_batches: merkle(124),
                observations: merkle(125),
                aggregates: merkle(126),
                rewards: merkle(127),
                availability: merkle(128),
                randomness_seed: fixed32(129),
                proposer_id: proposer,
                challenge_open_height: 0,
                challenge_deadline_height: 20,
            },
            signature: vec![2],
        })
        .unwrap();

    let challenge_id = fixed32(130);
    state
        .apply_open_challenge(OpenChallengeTx {
            challenge: Challenge {
                challenge_id,
                kind: pole_protocol_draft::ChallengeKind::BadStorage,
                epoch_id: 1,
                target_node: Some(target),
                challenger,
                bond: 50,
                opened_at_height: 0,
                deadline_height: 15,
                state: ChallengeState::Open,
                evidence: ChallengeEvidenceRef {
                    batch_root: None,
                    aggregate_root: None,
                    reward_root: None,
                    payload_cid: Some("cid://batch-4".into()),
                    merkle_proof: vec![fixed32(131)],
                },
            },
            signature: vec![3],
        })
        .unwrap();

    execute_block(
        &mut state,
        Block {
            height: 16,
            transactions: vec![],
        },
    )
    .unwrap();

    let err = state
        .apply_challenge_response(ChallengeResponseTx {
            challenge_id,
            responder: target,
            response_payload_cid: Some("cid://response".into()),
            response_hash: Some(fixed32(132)),
            signature: vec![4],
        })
        .unwrap_err();

    assert!(matches!(
        err,
        TransitionError::ChallengeResponseWindowElapsed {
            challenge_id: cid,
            current_height,
            deadline_height
        } if cid == challenge_id && current_height == 16 && deadline_height == 15
    ));
    assert!(state.store.open_challenge(&challenge_id).is_some());
    assert!(state.store.challenge_response(&challenge_id).is_none());
}

#[test]
fn persistent_store_stub_flushes() {
    let path =
        std::env::temp_dir().join(format!("pole-persistent-store-{}.bin", std::process::id()));
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }

    let mut store = pole_protocol_draft::PersistentStoreStub::open(&path).unwrap();
    store.insert_account(pole_protocol_draft::AccountState {
        address: fixed32(120),
        balance: 777,
        staked: 11,
        locked: 22,
        nonce: 3,
    });
    store.flush().unwrap();

    let reopened = pole_protocol_draft::PersistentStoreStub::open(&path).unwrap();
    let account = reopened.account(&fixed32(120)).unwrap();
    assert_eq!(account.balance, 777);
    assert_eq!(account.staked, 11);
    assert_eq!(account.locked, 22);
    assert_eq!(account.nonce, 3);

    std::fs::remove_file(path).unwrap();
}

#[test]
fn challenge_resolution_must_match_challenge_kind() {
    let params = test_params();
    let mut state = ProtocolState::new(params, 1, 1);

    let proposer = fixed32(121);
    let collector = fixed32(122);
    let target = fixed32(123);
    let challenger = fixed32(124);

    state.upsert_node(active_node(
        proposer,
        fixed32(125),
        10_000,
        vec![Capability::Propose],
    ));
    state.upsert_node(active_node(
        collector,
        fixed32(126),
        0,
        vec![Capability::Collect],
    ));
    state.upsert_node(active_node(
        target,
        fixed32(127),
        2_000,
        vec![Capability::Verify],
    ));
    state.upsert_account(pole_protocol_draft::AccountState {
        address: challenger,
        balance: 500,
        staked: 0,
        locked: 0,
        nonce: 0,
    });

    state
        .apply_submit_batch(SubmitBatchTx {
            batch_commit: BatchCommit {
                epoch_id: 1,
                collector_id: collector,
                slot_start: 1,
                slot_end: 1,
                batch: merkle(128),
                payload_cid: "cid://batch-4".into(),
                obs_count: 1,
                submitted_at_height: 0,
            },
            signature: vec![1],
        })
        .unwrap();
    state
        .apply_commit_epoch(CommitEpochTx {
            epoch_commit: EpochCommit {
                epoch_id: 1,
                accepted_batches: merkle(129),
                observations: merkle(130),
                aggregates: merkle(131),
                rewards: merkle(132),
                availability: merkle(133),
                randomness_seed: fixed32(134),
                proposer_id: proposer,
                challenge_open_height: 0,
                challenge_deadline_height: 20,
            },
            signature: vec![2],
        })
        .unwrap();

    let challenge_id = fixed32(135);
    state
        .apply_open_challenge(OpenChallengeTx {
            challenge: Challenge {
                challenge_id,
                kind: pole_protocol_draft::ChallengeKind::BadAggregate,
                epoch_id: 1,
                target_node: Some(target),
                challenger,
                bond: 50,
                opened_at_height: 0,
                deadline_height: 15,
                state: ChallengeState::Open,
                evidence: ChallengeEvidenceRef {
                    batch_root: None,
                    aggregate_root: Some(fixed32(131)),
                    reward_root: None,
                    payload_cid: None,
                    merkle_proof: vec![fixed32(136)],
                },
            },
            signature: vec![3],
        })
        .unwrap();

    let err = state
        .resolve_challenge(
            challenge_id,
            ChallengeResolution::BadStorage {
                slash_amount: 100,
                challenger_reward: 20,
                missing_payload_cid: Some("cid://wrong-kind"),
            },
        )
        .unwrap_err();

    assert!(matches!(
        err,
        TransitionError::ResolutionKindMismatch {
            challenge_id: cid,
            expected
        } if cid == challenge_id && expected == pole_protocol_draft::ChallengeKind::BadAggregate
    ));
}
