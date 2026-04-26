use crate::transactions::Transaction;
use crate::transitions::{ProtocolState, TransitionEffect, TransitionError};
use crate::ProtocolStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub height: u64,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockExecutionError {
    HeightRegression {
        current_height: u64,
        block_height: u64,
    },
    UnsupportedTransaction(&'static str),
    Transition(TransitionError),
}

impl From<TransitionError> for BlockExecutionError {
    fn from(value: TransitionError) -> Self {
        Self::Transition(value)
    }
}

pub fn execute_block<S: ProtocolStore>(
    state: &mut ProtocolState<S>,
    block: Block,
) -> Result<Vec<TransitionEffect>, BlockExecutionError> {
    if block.height <= state.height {
        return Err(BlockExecutionError::HeightRegression {
            current_height: state.height,
            block_height: block.height,
        });
    }

    state.height = block.height;
    let mut effects = state.process_mature_unbonds()?;
    effects.reserve(block.transactions.len());
    for tx in block.transactions {
        let effect = match tx {
            Transaction::Transfer(tx) => state.apply_transfer(tx)?,
            Transaction::Stake(tx) => state.apply_stake(tx)?,
            Transaction::Unbond(tx) => state.apply_unbond(tx)?,
            Transaction::SubmitBatch(tx) => state.apply_submit_batch(tx)?,
            Transaction::CommitEpoch(tx) => state.apply_commit_epoch(tx)?,
            Transaction::OpenChallenge(tx) => state.apply_open_challenge(tx)?,
            Transaction::ChallengeResponse(tx) => state.apply_challenge_response(tx)?,
            Transaction::ClaimReward(tx) => state.apply_claim_reward(tx)?,
            Transaction::Vote(tx) => state.apply_vote(tx)?,
            Transaction::ProposeProtocolParamsUpdate(tx) => {
                state.apply_propose_protocol_params_update(tx)?
            }
        };
        effects.push(effect);
    }

    Ok(effects)
}
