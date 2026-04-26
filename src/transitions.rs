use crate::params::{ProtocolParams, ProtocolParamsError};
use crate::primitives::{Address, Amount, Capability, EpochId, Hash32, Height, NodeId, NodeStatus};
use crate::records::{
    ChallengeResolutionDetails, ChallengeResolutionRecord, ChallengeResponseRecord,
    DelegationRecord, GovernanceParamsUpdateProposalRecord, GovernanceProposalKind,
    GovernanceProposalState, GovernanceVoteRecord, RewardRecord, UnbondingRecord,
};
use crate::state::{AccountState, NodeRegistry};
use crate::store::{BatchKey, DelegationKey, InMemoryStore, ProtocolStore, RewardKey, VoteKey};
use crate::transactions::{
    ChallengeResponseTx, ClaimRewardTx, CommitEpochTx, OpenChallengeTx,
    ProposeProtocolParamsUpdateTx, StakeTx, SubmitBatchTx, TransferTx, UnbondTx, VoteTx,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeResolution {
    BadBatch {
        slash_amount: Amount,
        challenger_reward: Amount,
        rejected_batch_root: Option<Hash32>,
    },
    Omission {
        slash_amount: Amount,
        challenger_reward: Amount,
        omitted_batch_root: Hash32,
    },
    BadAggregate {
        slash_amount: Amount,
        challenger_reward: Amount,
        corrected_aggregate_root: Option<Hash32>,
    },
    BadReward {
        slash_amount: Amount,
        challenger_reward: Amount,
        corrected_reward_root: Option<Hash32>,
    },
    BadStorage {
        slash_amount: Amount,
        challenger_reward: Amount,
        missing_payload_cid: Option<&'static str>,
    },
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolState<S = InMemoryStore> {
    pub height: Height,
    pub current_epoch: EpochId,
    pub params: ProtocolParams,
    pub store: S,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionEffect {
    BatchAccepted {
        key: BatchKey,
    },
    EpochCommitted {
        epoch_id: EpochId,
        challenge_deadline_height: Height,
    },
    ChallengeOpened {
        challenge_id: Hash32,
        locked_bond: Amount,
    },
    ChallengeResolved {
        challenge_id: Hash32,
        kind: crate::primitives::ChallengeKind,
        slash_amount: Amount,
        challenger_reward: Amount,
    },
    EpochFinalized {
        epoch_id: EpochId,
    },
    RewardClaimed {
        epoch_id: EpochId,
        node_id: NodeId,
        claimer: Address,
        amount: Amount,
    },
    TransferApplied {
        from: Address,
        to: Address,
        amount: Amount,
        fee: Amount,
    },
    StakeApplied {
        delegator: Address,
        operator: NodeId,
        amount: Amount,
    },
    UnbondQueued {
        delegator: Address,
        operator: NodeId,
        amount: Amount,
        unlock_height: Height,
    },
    UnbondMatured {
        delegator: Address,
        operator: NodeId,
        amount: Amount,
    },
    VoteRecorded {
        proposal_id: Hash32,
        voter: Address,
        voting_power: Amount,
    },
    ProtocolParamsUpdateProposed {
        proposal_id: Hash32,
        proposer: Address,
        effective_epoch: EpochId,
    },
    ChallengeResponded {
        challenge_id: Hash32,
        responder: NodeId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    EmptySignature,
    EmptyPayloadCid,
    EmptyBatch,
    InvalidSlotRange,
    StaleEpoch {
        epoch_id: EpochId,
        current_epoch: EpochId,
    },
    UnknownNode(NodeId),
    UnknownAccount(Address),
    NodeNotActive(NodeId),
    MissingCapability {
        node_id: NodeId,
        capability: Capability,
    },
    InsufficientBond {
        node_id: NodeId,
        required: Amount,
        actual: Amount,
    },
    DuplicateBatch(BatchKey),
    DuplicateEpochCommit(EpochId),
    MissingBatchForEpoch(EpochId),
    ChallengeWindowInvalid {
        open_height: Height,
        deadline_height: Height,
        max_window: u32,
    },
    EpochCommitNotFound(EpochId),
    ChallengeAlreadyExists(Hash32),
    ChallengeNotFound(Hash32),
    InvalidChallengeBond,
    MissingChallengeTarget(Hash32),
    InvalidChallengeReward {
        slash_amount: Amount,
        challenger_reward: Amount,
    },
    ResolutionKindMismatch {
        challenge_id: Hash32,
        expected: crate::primitives::ChallengeKind,
    },
    ChallengeStillOpen(EpochId),
    ChallengeWindowNotElapsed {
        current_height: Height,
        deadline_height: Height,
    },
    EpochAlreadyFinalized(EpochId),
    EpochNotFinalized(EpochId),
    RewardNotFound(RewardKey),
    RewardAlreadyClaimed(RewardKey),
    ClaimAmountMismatch {
        expected: Amount,
        actual: Amount,
    },
    InvalidRewardClaimer {
        expected: Address,
        actual: Address,
    },
    InvalidAmount,
    DelegationNotFound(DelegationKey),
    InsufficientDelegation {
        key: DelegationKey,
        required: Amount,
        actual: Amount,
    },
    InsufficientVotingPower {
        address: Address,
        required: Amount,
        available: Amount,
    },
    DuplicateVote(VoteKey),
    ChallengeAlreadyResponded(Hash32),
    ChallengeResponseWindowElapsed {
        challenge_id: Hash32,
        current_height: Height,
        deadline_height: Height,
    },
    DuplicateParamsUpdateProposal(Hash32),
    ParamsUpdateProposalNotFound(Hash32),
    ParamsUpdateMustTargetFutureEpoch {
        current_epoch: EpochId,
        effective_epoch: EpochId,
    },
    InvalidChallengeResponder {
        challenge_id: Hash32,
        expected: NodeId,
        actual: NodeId,
    },
    EmptyChallengeResponse,
    NonceMismatch {
        address: Address,
        expected: u64,
        actual: u64,
    },
    InsufficientBalance {
        address: Address,
        required: Amount,
        available: Amount,
    },
    ParamsSerialization(String),
    InvalidProtocolParams(ProtocolParamsError),
}

impl ProtocolState<InMemoryStore> {
    pub fn new(params: ProtocolParams, height: Height, current_epoch: EpochId) -> Self {
        params
            .validate()
            .expect("protocol params must be valid before creating ProtocolState");
        Self {
            height,
            current_epoch,
            params,
            store: InMemoryStore::default(),
        }
    }
}

impl<S: ProtocolStore> ProtocolState<S> {
    pub fn with_store(
        params: ProtocolParams,
        height: Height,
        current_epoch: EpochId,
        store: S,
    ) -> Self {
        params
            .validate()
            .expect("protocol params must be valid before creating ProtocolState");
        Self {
            height,
            current_epoch,
            params,
            store,
        }
    }

    pub fn apply_submit_batch(
        &mut self,
        tx: SubmitBatchTx,
    ) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;

        let mut batch = tx.batch_commit;
        ensure_epoch_not_stale(batch.epoch_id, self.current_epoch)?;
        ensure_non_empty_payload(&batch.payload_cid)?;
        if batch.obs_count == 0 {
            return Err(TransitionError::EmptyBatch);
        }
        if batch.slot_start > batch.slot_end {
            return Err(TransitionError::InvalidSlotRange);
        }

        let node =
            self.require_active_node_with_capability(&batch.collector_id, Capability::Collect)?;
        let key = (batch.epoch_id, node.node_id, batch.batch.root);
        if self.store.batch(&key).is_some() {
            return Err(TransitionError::DuplicateBatch(key));
        }

        // Canonicalize the height at which this batch becomes part of state.
        batch.submitted_at_height = self.height;
        self.store.insert_batch(key, batch);
        Ok(TransitionEffect::BatchAccepted { key })
    }

    pub fn apply_commit_epoch(
        &mut self,
        tx: CommitEpochTx,
    ) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;

        let mut commit = tx.epoch_commit;
        ensure_epoch_not_stale(commit.epoch_id, self.current_epoch)?;
        let proposer =
            self.require_active_node_with_capability(&commit.proposer_id, Capability::Propose)?;
        ensure_bond(
            proposer.node_id,
            proposer.bond,
            self.params.min_propose_bond,
        )?;

        if self.store.epoch_commit(&commit.epoch_id).is_some() {
            return Err(TransitionError::DuplicateEpochCommit(commit.epoch_id));
        }
        if !self.store.has_any_batch_for_epoch(commit.epoch_id) {
            return Err(TransitionError::MissingBatchForEpoch(commit.epoch_id));
        }

        let open_height = self.height;
        if commit.challenge_deadline_height <= open_height
            || commit.challenge_deadline_height - open_height
                > self.params.challenge_window_blocks as u64
        {
            return Err(TransitionError::ChallengeWindowInvalid {
                open_height,
                deadline_height: commit.challenge_deadline_height,
                max_window: self.params.challenge_window_blocks,
            });
        }

        // Canonicalize when the epoch enters its challenge period.
        commit.challenge_open_height = open_height;
        self.store
            .insert_epoch_commit(commit.epoch_id, commit.clone());
        Ok(TransitionEffect::EpochCommitted {
            epoch_id: commit.epoch_id,
            challenge_deadline_height: commit.challenge_deadline_height,
        })
    }

    pub fn apply_open_challenge(
        &mut self,
        tx: OpenChallengeTx,
    ) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;

        let mut challenge = tx.challenge;
        if challenge.bond == 0 {
            return Err(TransitionError::InvalidChallengeBond);
        }
        if self.store.open_challenge(&challenge.challenge_id).is_some() {
            return Err(TransitionError::ChallengeAlreadyExists(
                challenge.challenge_id,
            ));
        }

        let challenge_window_deadline = self
            .store
            .epoch_commit(&challenge.epoch_id)
            .ok_or(TransitionError::EpochCommitNotFound(challenge.epoch_id))?
            .challenge_deadline_height;
        if challenge.deadline_height <= self.height
            || challenge.deadline_height > challenge_window_deadline
        {
            return Err(TransitionError::ChallengeWindowInvalid {
                open_height: self.height,
                deadline_height: challenge.deadline_height,
                max_window: self.params.challenge_window_blocks,
            });
        }

        if let Some(target) = challenge.target_node {
            self.require_active_node(target)?;
        }

        let challenger_account = self
            .store
            .account_mut(&challenge.challenger)
            .ok_or(TransitionError::UnknownAccount(challenge.challenger))?;
        lock_bond(challenger_account, challenge.bond)?;

        // Canonicalize the challenge opening height. Proof validation and
        // recomputation are deferred to a later challenge resolution transition.
        challenge.opened_at_height = self.height;
        self.store
            .insert_open_challenge(challenge.challenge_id, challenge.clone());

        Ok(TransitionEffect::ChallengeOpened {
            challenge_id: challenge.challenge_id,
            locked_bond: challenge.bond,
        })
    }

    pub fn resolve_challenge(
        &mut self,
        challenge_id: Hash32,
        resolution: ChallengeResolution,
    ) -> Result<TransitionEffect, TransitionError> {
        let mut challenge = self
            .store
            .remove_open_challenge(&challenge_id)
            .ok_or(TransitionError::ChallengeNotFound(challenge_id))?;

        match resolution {
            ChallengeResolution::Rejected => {
                let challenger = self
                    .store
                    .account_mut(&challenge.challenger)
                    .ok_or(TransitionError::UnknownAccount(challenge.challenger))?;
                burn_locked_bond(challenger, challenge.bond)?;

                challenge.state = crate::primitives::ChallengeState::Rejected;
                self.store
                    .insert_resolved_challenge(challenge.challenge_id, challenge.clone());
                self.store.insert_challenge_resolution(
                    challenge.challenge_id,
                    ChallengeResolutionRecord {
                        challenge_id: challenge.challenge_id,
                        kind: challenge.kind,
                        slash_amount: 0,
                        challenger_reward: 0,
                        details: ChallengeResolutionDetails::Rejected,
                        resolved_at_height: self.height,
                    },
                );

                Ok(TransitionEffect::ChallengeResolved {
                    challenge_id: challenge.challenge_id,
                    kind: challenge.kind,
                    slash_amount: 0,
                    challenger_reward: 0,
                })
            }
            resolution => {
                let (slash_amount, challenger_reward) = resolution_amounts(resolution);
                let details =
                    resolution_details(challenge.challenge_id, challenge.kind, resolution)?;
                if challenger_reward > slash_amount {
                    return Err(TransitionError::InvalidChallengeReward {
                        slash_amount,
                        challenger_reward,
                    });
                }

                let target_node_id =
                    challenge
                        .target_node
                        .ok_or(TransitionError::MissingChallengeTarget(
                            challenge.challenge_id,
                        ))?;
                {
                    let target_node = self
                        .store
                        .node_mut(&target_node_id)
                        .ok_or(TransitionError::UnknownNode(target_node_id))?;
                    ensure_bond(target_node.node_id, target_node.bond, slash_amount)?;
                    target_node.bond -= slash_amount;
                    target_node.reputation.challengeable_faults += 1;
                }

                let challenger = self
                    .store
                    .account_mut(&challenge.challenger)
                    .ok_or(TransitionError::UnknownAccount(challenge.challenger))?;
                unlock_bond_to_balance(
                    challenger,
                    challenge.bond,
                    challenge.bond + challenger_reward,
                )?;

                challenge.state = crate::primitives::ChallengeState::Succeeded;
                self.store
                    .insert_resolved_challenge(challenge.challenge_id, challenge.clone());
                self.store.insert_challenge_resolution(
                    challenge.challenge_id,
                    ChallengeResolutionRecord {
                        challenge_id: challenge.challenge_id,
                        kind: challenge.kind,
                        slash_amount,
                        challenger_reward,
                        details,
                        resolved_at_height: self.height,
                    },
                );

                Ok(TransitionEffect::ChallengeResolved {
                    challenge_id: challenge.challenge_id,
                    kind: challenge.kind,
                    slash_amount,
                    challenger_reward,
                })
            }
        }
    }

    pub fn finalize_epoch(
        &mut self,
        epoch_id: EpochId,
    ) -> Result<TransitionEffect, TransitionError> {
        if self.store.is_epoch_finalized(epoch_id) {
            return Err(TransitionError::EpochAlreadyFinalized(epoch_id));
        }
        let deadline_height = self
            .store
            .epoch_commit(&epoch_id)
            .ok_or(TransitionError::EpochCommitNotFound(epoch_id))?
            .challenge_deadline_height;

        if self.height <= deadline_height {
            return Err(TransitionError::ChallengeWindowNotElapsed {
                current_height: self.height,
                deadline_height,
            });
        }
        if self.store.has_open_challenges_for_epoch(epoch_id) {
            return Err(TransitionError::ChallengeStillOpen(epoch_id));
        }

        self.store.mark_epoch_finalized(epoch_id);
        if self.current_epoch <= epoch_id {
            self.current_epoch = epoch_id + 1;
            self.activate_scheduled_protocol_params();
            self.expire_stale_params_update_proposals()?;
        }

        Ok(TransitionEffect::EpochFinalized { epoch_id })
    }

    pub fn schedule_protocol_params_update(
        &mut self,
        effective_epoch: EpochId,
        params: ProtocolParams,
    ) -> Result<(), TransitionError> {
        params
            .validate()
            .map_err(TransitionError::InvalidProtocolParams)?;
        if effective_epoch <= self.current_epoch {
            return Err(TransitionError::ParamsUpdateMustTargetFutureEpoch {
                current_epoch: self.current_epoch,
                effective_epoch,
            });
        }
        self.store
            .insert_scheduled_protocol_params(effective_epoch, params);
        Ok(())
    }

    pub fn apply_claim_reward(
        &mut self,
        tx: ClaimRewardTx,
    ) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;

        if !self.store.is_epoch_finalized(tx.epoch_id) {
            return Err(TransitionError::EpochNotFinalized(tx.epoch_id));
        }

        let reward_key = (tx.epoch_id, tx.node_id);
        if self.store.is_reward_claimed(&reward_key) {
            return Err(TransitionError::RewardAlreadyClaimed(reward_key));
        }

        let reward = self
            .store
            .reward_record(&reward_key)
            .cloned()
            .ok_or(TransitionError::RewardNotFound(reward_key))?;
        let reward_address = self
            .store
            .node(&tx.node_id)
            .ok_or(TransitionError::UnknownNode(tx.node_id))?
            .reward_address;
        if reward_address != tx.claimer {
            return Err(TransitionError::InvalidRewardClaimer {
                expected: reward_address,
                actual: tx.claimer,
            });
        }
        if reward.net_reward != tx.amount {
            return Err(TransitionError::ClaimAmountMismatch {
                expected: reward.net_reward,
                actual: tx.amount,
            });
        }

        let account = self
            .store
            .account_mut(&tx.claimer)
            .ok_or(TransitionError::UnknownAccount(tx.claimer))?;
        account.balance += tx.amount;
        self.store.mark_reward_claimed(reward_key);

        Ok(TransitionEffect::RewardClaimed {
            epoch_id: tx.epoch_id,
            node_id: tx.node_id,
            claimer: tx.claimer,
            amount: tx.amount,
        })
    }

    pub fn apply_transfer(&mut self, tx: TransferTx) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;

        let sender = self
            .store
            .account_mut(&tx.from)
            .ok_or(TransitionError::UnknownAccount(tx.from))?;
        ensure_nonce(sender.address, sender.nonce, tx.nonce)?;

        let total = tx.amount.saturating_add(tx.fee);
        if sender.balance < total {
            return Err(TransitionError::InsufficientBalance {
                address: sender.address,
                required: total,
                available: sender.balance,
            });
        }

        sender.balance -= total;
        sender.nonce += 1;

        let recipient = self
            .store
            .account_mut(&tx.to)
            .ok_or(TransitionError::UnknownAccount(tx.to))?;
        recipient.balance += tx.amount;

        Ok(TransitionEffect::TransferApplied {
            from: tx.from,
            to: tx.to,
            amount: tx.amount,
            fee: tx.fee,
        })
    }

    pub fn apply_stake(&mut self, tx: StakeTx) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;
        ensure_positive_amount(tx.amount)?;
        self.require_active_node(tx.operator)?;

        let delegator = self
            .store
            .account_mut(&tx.delegator)
            .ok_or(TransitionError::UnknownAccount(tx.delegator))?;
        ensure_nonce(delegator.address, delegator.nonce, tx.nonce)?;
        if delegator.balance < tx.amount {
            return Err(TransitionError::InsufficientBalance {
                address: delegator.address,
                required: tx.amount,
                available: delegator.balance,
            });
        }
        delegator.balance -= tx.amount;
        delegator.staked += tx.amount;
        delegator.nonce += 1;

        let operator = self
            .store
            .node_mut(&tx.operator)
            .ok_or(TransitionError::UnknownNode(tx.operator))?;
        operator.bond += tx.amount;

        let key = (tx.delegator, tx.operator);
        let next_amount = self
            .store
            .delegation(&key)
            .map(|record| record.amount)
            .unwrap_or(0)
            + tx.amount;
        self.store.insert_delegation(
            key,
            DelegationRecord {
                delegator: tx.delegator,
                operator: tx.operator,
                amount: next_amount,
            },
        );

        Ok(TransitionEffect::StakeApplied {
            delegator: tx.delegator,
            operator: tx.operator,
            amount: tx.amount,
        })
    }

    pub fn apply_unbond(&mut self, tx: UnbondTx) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;
        ensure_positive_amount(tx.amount)?;

        let key = (tx.delegator, tx.operator);
        let existing = self
            .store
            .delegation(&key)
            .cloned()
            .ok_or(TransitionError::DelegationNotFound(key))?;
        if existing.amount < tx.amount {
            return Err(TransitionError::InsufficientDelegation {
                key,
                required: tx.amount,
                actual: existing.amount,
            });
        }

        let delegator = self
            .store
            .account_mut(&tx.delegator)
            .ok_or(TransitionError::UnknownAccount(tx.delegator))?;
        ensure_nonce(delegator.address, delegator.nonce, tx.nonce)?;
        if delegator.staked < tx.amount {
            return Err(TransitionError::InsufficientBalance {
                address: delegator.address,
                required: tx.amount,
                available: delegator.staked,
            });
        }
        delegator.staked -= tx.amount;
        delegator.locked += tx.amount;
        delegator.nonce += 1;

        let operator = self
            .store
            .node_mut(&tx.operator)
            .ok_or(TransitionError::UnknownNode(tx.operator))?;
        ensure_bond(operator.node_id, operator.bond, tx.amount)?;
        operator.bond -= tx.amount;

        let remaining = existing.amount - tx.amount;
        if remaining == 0 {
            self.store.remove_delegation(&key);
        } else {
            self.store.insert_delegation(
                key,
                DelegationRecord {
                    delegator: tx.delegator,
                    operator: tx.operator,
                    amount: remaining,
                },
            );
        }

        let unlock_height = self.height + self.params.unbonding_blocks as u64;
        self.store.queue_unbonding(UnbondingRecord {
            delegator: tx.delegator,
            operator: tx.operator,
            amount: tx.amount,
            unlock_height,
        });

        Ok(TransitionEffect::UnbondQueued {
            delegator: tx.delegator,
            operator: tx.operator,
            amount: tx.amount,
            unlock_height,
        })
    }

    pub fn apply_vote(&mut self, tx: VoteTx) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;
        ensure_positive_amount(tx.voting_power)?;

        let key = (tx.proposal_id, tx.voter);
        if self.store.vote_record(&key).is_some() {
            return Err(TransitionError::DuplicateVote(key));
        }

        let voter = self
            .store
            .account_mut(&tx.voter)
            .ok_or(TransitionError::UnknownAccount(tx.voter))?;
        ensure_nonce(voter.address, voter.nonce, tx.nonce)?;
        let available_voting_power = voter.balance + voter.staked + voter.locked;
        if tx.voting_power > available_voting_power {
            return Err(TransitionError::InsufficientVotingPower {
                address: voter.address,
                required: tx.voting_power,
                available: available_voting_power,
            });
        }
        voter.nonce += 1;

        self.store.insert_vote_record(
            key,
            GovernanceVoteRecord {
                proposal_id: tx.proposal_id,
                voter: tx.voter,
                choice: tx.choice,
                voting_power: tx.voting_power,
                recorded_height: self.height,
            },
        );

        self.try_schedule_params_update_after_vote(tx.proposal_id)?;

        Ok(TransitionEffect::VoteRecorded {
            proposal_id: tx.proposal_id,
            voter: tx.voter,
            voting_power: tx.voting_power,
        })
    }

    pub fn apply_propose_protocol_params_update(
        &mut self,
        tx: ProposeProtocolParamsUpdateTx,
    ) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;
        if self.store.params_update_proposal(&tx.proposal_id).is_some() {
            return Err(TransitionError::DuplicateParamsUpdateProposal(
                tx.proposal_id,
            ));
        }
        let proposer = self
            .store
            .account_mut(&tx.proposer)
            .ok_or(TransitionError::UnknownAccount(tx.proposer))?;
        ensure_nonce(proposer.address, proposer.nonce, tx.nonce)?;
        let proposal_kind = classify_protocol_params_update(&self.params, &tx.params);
        let required_bond = match proposal_kind {
            GovernanceProposalKind::FastParams => self.params.governance.params_update_bond,
            GovernanceProposalKind::SlowParams => self.params.governance.slow_params_update_bond,
        };
        lock_bond(proposer, required_bond)?;
        proposer.nonce += 1;

        let params_hash = crate::stable_hash32(
            &borsh::to_vec(&tx.params)
                .map_err(|err| TransitionError::ParamsSerialization(err.to_string()))?,
        );
        self.store.insert_params_update_proposal(
            tx.proposal_id,
            GovernanceParamsUpdateProposalRecord {
                proposal_id: tx.proposal_id,
                proposer: tx.proposer,
                kind: proposal_kind,
                effective_epoch: tx.effective_epoch,
                submitted_height: self.height,
                bond_amount: required_bond,
                params_hash,
                params: tx.params,
                state: GovernanceProposalState::Pending,
            },
        );

        Ok(TransitionEffect::ProtocolParamsUpdateProposed {
            proposal_id: tx.proposal_id,
            proposer: tx.proposer,
            effective_epoch: tx.effective_epoch,
        })
    }

    fn activate_scheduled_protocol_params(&mut self) {
        if let Some(params) = self
            .store
            .take_scheduled_protocol_params(&self.current_epoch)
        {
            self.params = params;
            let proposals = self.store.params_update_proposals_iter();
            for proposal in proposals {
                if proposal.state == GovernanceProposalState::Scheduled
                    && proposal.effective_epoch == self.current_epoch
                {
                    if let Some(record) =
                        self.store.params_update_proposal_mut(&proposal.proposal_id)
                    {
                        record.state = GovernanceProposalState::Activated;
                    }
                }
            }
        }
    }

    fn try_schedule_params_update_after_vote(
        &mut self,
        proposal_id: Hash32,
    ) -> Result<(), TransitionError> {
        let Some(proposal) = self.store.params_update_proposal(&proposal_id).cloned() else {
            return Ok(());
        };
        if proposal.state != GovernanceProposalState::Pending {
            return Ok(());
        }

        let mut yes = 0u128;
        let mut no = 0u128;
        let mut abstain = 0u128;
        let mut total_votes = 0u32;
        for vote in self.store.vote_records_for_proposal(&proposal_id) {
            total_votes += 1;
            match vote.choice {
                crate::primitives::VoteChoice::Yes => yes += vote.voting_power,
                crate::primitives::VoteChoice::No => no += vote.voting_power,
                crate::primitives::VoteChoice::Abstain => abstain += vote.voting_power,
            }
        }

        if total_votes == 0 {
            return Ok(());
        }

        let participating_power = yes + no + abstain;
        let total_voting_power = self
            .store
            .accounts_iter()
            .into_iter()
            .map(|account| account.balance + account.staked + account.locked)
            .sum::<u128>();
        if total_voting_power == 0 {
            return Ok(());
        }

        let quorum_bps = participating_power.saturating_mul(10_000) / total_voting_power;
        let required_quorum_bps = match proposal.kind {
            GovernanceProposalKind::FastParams => self.params.governance.params_update_quorum_bps,
            GovernanceProposalKind::SlowParams => {
                self.params.governance.slow_params_update_quorum_bps
            }
        };
        if quorum_bps < u128::from(required_quorum_bps) {
            return Ok(());
        }

        let decisive_power = yes + no;
        if decisive_power == 0 {
            return Ok(());
        }
        let approval_bps = yes.saturating_mul(10_000) / decisive_power;
        let required_approval_bps = match proposal.kind {
            GovernanceProposalKind::FastParams => self.params.governance.params_update_approval_bps,
            GovernanceProposalKind::SlowParams => {
                self.params.governance.slow_params_update_approval_bps
            }
        };
        if approval_bps < u128::from(required_approval_bps) {
            return Ok(());
        }

        self.schedule_protocol_params_update(proposal.effective_epoch, proposal.params.clone())?;
        let proposer_account = self
            .store
            .account_mut(&proposal.proposer)
            .ok_or(TransitionError::UnknownAccount(proposal.proposer))?;
        unlock_bond_to_balance(proposer_account, proposal.bond_amount, proposal.bond_amount)?;
        let proposal_record = self
            .store
            .params_update_proposal_mut(&proposal_id)
            .ok_or(TransitionError::ParamsUpdateProposalNotFound(proposal_id))?;
        proposal_record.state = GovernanceProposalState::Scheduled;
        Ok(())
    }

    fn expire_stale_params_update_proposals(&mut self) -> Result<(), TransitionError> {
        let proposals = self.store.params_update_proposals_iter();
        for proposal in proposals {
            if proposal.state != GovernanceProposalState::Pending
                || proposal.effective_epoch > self.current_epoch
            {
                continue;
            }
            let proposer_account = self
                .store
                .account_mut(&proposal.proposer)
                .ok_or(TransitionError::UnknownAccount(proposal.proposer))?;
            burn_locked_bond(proposer_account, proposal.bond_amount)?;
            let proposal_record = self
                .store
                .params_update_proposal_mut(&proposal.proposal_id)
                .ok_or(TransitionError::ParamsUpdateProposalNotFound(
                    proposal.proposal_id,
                ))?;
            proposal_record.state = GovernanceProposalState::Expired;
        }
        Ok(())
    }

    pub fn apply_challenge_response(
        &mut self,
        tx: ChallengeResponseTx,
    ) -> Result<TransitionEffect, TransitionError> {
        ensure_signature(&tx.signature)?;
        if tx.response_payload_cid.is_none() && tx.response_hash.is_none() {
            return Err(TransitionError::EmptyChallengeResponse);
        }

        if self.store.challenge_response(&tx.challenge_id).is_some() {
            return Err(TransitionError::ChallengeAlreadyResponded(tx.challenge_id));
        }

        let mut challenge = self
            .store
            .open_challenge(&tx.challenge_id)
            .cloned()
            .ok_or(TransitionError::ChallengeNotFound(tx.challenge_id))?;

        if self.height > challenge.deadline_height {
            return Err(TransitionError::ChallengeResponseWindowElapsed {
                challenge_id: tx.challenge_id,
                current_height: self.height,
                deadline_height: challenge.deadline_height,
            });
        }

        if let Some(expected) = challenge.target_node {
            if expected != tx.responder {
                return Err(TransitionError::InvalidChallengeResponder {
                    challenge_id: tx.challenge_id,
                    expected,
                    actual: tx.responder,
                });
            }
        } else {
            return Err(TransitionError::MissingChallengeTarget(tx.challenge_id));
        }

        challenge.state = crate::primitives::ChallengeState::Responded;
        self.store.remove_open_challenge(&tx.challenge_id);
        self.store
            .insert_open_challenge(tx.challenge_id, challenge.clone());
        self.store.insert_challenge_response(
            tx.challenge_id,
            ChallengeResponseRecord {
                challenge_id: tx.challenge_id,
                responder: tx.responder,
                response_payload_cid: tx.response_payload_cid,
                response_hash: tx.response_hash,
                responded_at_height: self.height,
            },
        );

        Ok(TransitionEffect::ChallengeResponded {
            challenge_id: tx.challenge_id,
            responder: tx.responder,
        })
    }

    pub fn process_mature_unbonds(&mut self) -> Result<Vec<TransitionEffect>, TransitionError> {
        let matured = self.store.drain_mature_unbondings(self.height);
        let mut effects = Vec::with_capacity(matured.len());

        for request in matured {
            let delegator = self
                .store
                .account_mut(&request.delegator)
                .ok_or(TransitionError::UnknownAccount(request.delegator))?;
            unlock_bond_to_balance(delegator, request.amount, request.amount)?;
            effects.push(TransitionEffect::UnbondMatured {
                delegator: request.delegator,
                operator: request.operator,
                amount: request.amount,
            });
        }

        Ok(effects)
    }

    pub fn upsert_account(&mut self, account: AccountState) {
        self.store.insert_account(account);
    }

    pub fn upsert_node(&mut self, node: NodeRegistry) {
        self.store.insert_node(node);
    }

    pub fn upsert_reward_record(&mut self, reward: RewardRecord) {
        self.store
            .insert_reward_record((reward.epoch_id, reward.node_id), reward);
    }

    fn require_active_node(&self, node_id: NodeId) -> Result<&NodeRegistry, TransitionError> {
        let node = self
            .store
            .node(&node_id)
            .ok_or(TransitionError::UnknownNode(node_id))?;
        if node.status != NodeStatus::Active {
            return Err(TransitionError::NodeNotActive(node_id));
        }
        Ok(node)
    }

    fn require_active_node_with_capability(
        &self,
        node_id: &NodeId,
        capability: Capability,
    ) -> Result<&NodeRegistry, TransitionError> {
        let node = self.require_active_node(*node_id)?;
        if !node.enabled_capabilities.contains(&capability) {
            return Err(TransitionError::MissingCapability {
                node_id: *node_id,
                capability,
            });
        }
        Ok(node)
    }
}

fn ensure_signature(signature: &[u8]) -> Result<(), TransitionError> {
    if signature.is_empty() {
        return Err(TransitionError::EmptySignature);
    }
    Ok(())
}

fn ensure_non_empty_payload(payload_cid: &str) -> Result<(), TransitionError> {
    if payload_cid.is_empty() {
        return Err(TransitionError::EmptyPayloadCid);
    }
    Ok(())
}

fn ensure_epoch_not_stale(
    epoch_id: EpochId,
    current_epoch: EpochId,
) -> Result<(), TransitionError> {
    if epoch_id < current_epoch {
        return Err(TransitionError::StaleEpoch {
            epoch_id,
            current_epoch,
        });
    }
    Ok(())
}

fn ensure_positive_amount(amount: Amount) -> Result<(), TransitionError> {
    if amount == 0 {
        return Err(TransitionError::InvalidAmount);
    }
    Ok(())
}

fn ensure_nonce(address: Address, expected: u64, actual: u64) -> Result<(), TransitionError> {
    if expected != actual {
        return Err(TransitionError::NonceMismatch {
            address,
            expected,
            actual,
        });
    }
    Ok(())
}

fn ensure_bond(node_id: NodeId, actual: Amount, required: Amount) -> Result<(), TransitionError> {
    if actual < required {
        return Err(TransitionError::InsufficientBond {
            node_id,
            required,
            actual,
        });
    }
    Ok(())
}

fn lock_bond(account: &mut AccountState, amount: Amount) -> Result<(), TransitionError> {
    if account.balance < amount {
        return Err(TransitionError::InsufficientBalance {
            address: account.address,
            required: amount,
            available: account.balance,
        });
    }
    account.balance -= amount;
    account.locked += amount;
    Ok(())
}

fn unlock_bond_to_balance(
    account: &mut AccountState,
    locked_amount: Amount,
    credit_amount: Amount,
) -> Result<(), TransitionError> {
    if account.locked < locked_amount {
        return Err(TransitionError::InsufficientBalance {
            address: account.address,
            required: locked_amount,
            available: account.locked,
        });
    }
    account.locked -= locked_amount;
    account.balance += credit_amount;
    Ok(())
}

fn burn_locked_bond(account: &mut AccountState, amount: Amount) -> Result<(), TransitionError> {
    if account.locked < amount {
        return Err(TransitionError::InsufficientBalance {
            address: account.address,
            required: amount,
            available: account.locked,
        });
    }
    account.locked -= amount;
    Ok(())
}

fn classify_protocol_params_update(
    current: &ProtocolParams,
    proposed: &ProtocolParams,
) -> GovernanceProposalKind {
    if current.rewards.reward_block_secs != proposed.rewards.reward_block_secs {
        GovernanceProposalKind::SlowParams
    } else {
        GovernanceProposalKind::FastParams
    }
}

fn resolution_details(
    challenge_id: Hash32,
    expected_kind: crate::primitives::ChallengeKind,
    resolution: ChallengeResolution,
) -> Result<ChallengeResolutionDetails, TransitionError> {
    use crate::primitives::ChallengeKind;

    match (expected_kind, resolution) {
        (
            ChallengeKind::BadBatch,
            ChallengeResolution::BadBatch {
                rejected_batch_root,
                ..
            },
        ) => Ok(ChallengeResolutionDetails::BadBatch {
            rejected_batch_root,
        }),
        (
            ChallengeKind::Omission,
            ChallengeResolution::Omission {
                omitted_batch_root, ..
            },
        ) => Ok(ChallengeResolutionDetails::Omission { omitted_batch_root }),
        (
            ChallengeKind::BadAggregate,
            ChallengeResolution::BadAggregate {
                corrected_aggregate_root,
                ..
            },
        ) => Ok(ChallengeResolutionDetails::BadAggregate {
            corrected_aggregate_root,
        }),
        (
            ChallengeKind::BadReward,
            ChallengeResolution::BadReward {
                corrected_reward_root,
                ..
            },
        ) => Ok(ChallengeResolutionDetails::BadReward {
            corrected_reward_root,
        }),
        (
            ChallengeKind::BadStorage,
            ChallengeResolution::BadStorage {
                missing_payload_cid,
                ..
            },
        ) => Ok(ChallengeResolutionDetails::BadStorage {
            missing_payload_cid: missing_payload_cid.map(str::to_string),
        }),
        (_, ChallengeResolution::Rejected) => Ok(ChallengeResolutionDetails::Rejected),
        _ => Err(TransitionError::ResolutionKindMismatch {
            challenge_id,
            expected: expected_kind,
        }),
    }
}

fn resolution_amounts(resolution: ChallengeResolution) -> (Amount, Amount) {
    match resolution {
        ChallengeResolution::BadBatch {
            slash_amount,
            challenger_reward,
            ..
        }
        | ChallengeResolution::Omission {
            slash_amount,
            challenger_reward,
            ..
        }
        | ChallengeResolution::BadAggregate {
            slash_amount,
            challenger_reward,
            ..
        }
        | ChallengeResolution::BadReward {
            slash_amount,
            challenger_reward,
            ..
        }
        | ChallengeResolution::BadStorage {
            slash_amount,
            challenger_reward,
            ..
        } => (slash_amount, challenger_reward),
        ChallengeResolution::Rejected => (0, 0),
    }
}
