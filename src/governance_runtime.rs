use std::io;

use crate::ProtocolStore;
use crate::{
    execute_block, export_governance_artifacts, local_chain_runtime_path,
    open_local_protocol_state, Block, NodeConfig, ProposeProtocolParamsUpdateTx, ProtocolParams,
    TransitionEffect, VoteChoice, VoteTx,
};

pub fn execute_governance_vote(
    config: &NodeConfig,
    proposal_id: [u8; 32],
    choice: VoteChoice,
    voting_power: u128,
) -> Result<(Vec<TransitionEffect>, bool), Box<dyn std::error::Error>> {
    let (mut runtime, mut state) =
        open_local_protocol_state(config, config.runtime.challenge_window_blocks)?;
    let voter = config.reward_address()?;
    let block_height = state.height.saturating_add(1).max(1);
    let nonce = state
        .store
        .account(&voter)
        .map(|account| account.nonce)
        .unwrap_or(0);
    let effects = execute_block(
        &mut state,
        Block {
            height: block_height,
            transactions: vec![crate::Transaction::Vote(VoteTx {
                proposal_id,
                voter,
                choice,
                voting_power,
                nonce,
                signature: vec![1],
            })],
        },
    )
    .map_err(|err| io::Error::other(format!("governance vote execution failed: {err:?}")))?;
    let scheduled = state
        .store
        .scheduled_protocol_params(&state.current_epoch.saturating_add(1))
        .is_some();
    runtime.height = state.height;
    runtime.current_epoch = state.current_epoch;
    runtime.save_json(local_chain_runtime_path(config))?;
    state.store.flush()?;
    let _ = export_governance_artifacts(config, &state.store, state.current_epoch)?;

    Ok((effects, scheduled))
}

pub fn submit_protocol_params_update_proposal(
    config: &NodeConfig,
    proposal_id: [u8; 32],
    effective_epoch: u64,
    params: ProtocolParams,
) -> Result<Vec<TransitionEffect>, Box<dyn std::error::Error>> {
    params
        .validate()
        .map_err(|err| io::Error::other(format!("invalid proposed params: {err}")))?;

    let (mut runtime, mut state) =
        open_local_protocol_state(config, config.runtime.challenge_window_blocks)?;
    let proposer = config.reward_address()?;
    let block_height = state.height.saturating_add(1).max(1);
    let nonce = state
        .store
        .account(&proposer)
        .map(|account| account.nonce)
        .unwrap_or(0);
    let effects = execute_block(
        &mut state,
        Block {
            height: block_height,
            transactions: vec![crate::Transaction::ProposeProtocolParamsUpdate(
                ProposeProtocolParamsUpdateTx {
                    proposal_id,
                    proposer,
                    effective_epoch,
                    params,
                    nonce,
                    signature: vec![1],
                },
            )],
        },
    )
    .map_err(|err| io::Error::other(format!("governance proposal execution failed: {err:?}")))?;
    runtime.height = state.height;
    runtime.current_epoch = state.current_epoch;
    runtime.save_json(local_chain_runtime_path(config))?;
    state.store.flush()?;
    let _ = export_governance_artifacts(config, &state.store, state.current_epoch)?;

    Ok(effects)
}
