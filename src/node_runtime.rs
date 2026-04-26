use std::fmt;

use crate::node_config::{NodeConfig, NodeConfigError};
use crate::node_pipeline::{
    merkle_root, stable_hash32, AssembledBatch, BatchBuilder, NodePipelineError,
    SteamCurrentPlayersSample,
};
use crate::node_rewards::effective_min_retention_epochs;
use crate::p2p::{
    batch_announcement_from_assembled, replica_receipt_announcement_from_record, P2pError,
    P2pMessage, P2pNetwork,
};
use crate::primitives::{Capability, EpochId, Hash32, Height, NodeId};
use crate::records::{EpochCommit, ObservationRecord};
use crate::storage_book::{LocalRetentionBook, StorageBookError, StoredPayloadRecord};
use crate::MerkleCommitment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectAndStoreOutcome {
    pub assembled_batch: AssembledBatch,
    pub stored_payload: Option<StoredPayloadRecord>,
    pub batch_recipients: usize,
    pub receipt_recipients: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct EpochCommitInputs<'a> {
    pub epoch_id: EpochId,
    pub current_height: Height,
    pub challenge_window_blocks: u32,
    pub batches: &'a [AssembledBatch],
    pub stored_payloads: &'a [StoredPayloadRecord],
    pub aggregates_root: Hash32,
    pub rewards_root: Hash32,
}

#[derive(Debug)]
pub enum NodeRuntimeError {
    Config(NodeConfigError),
    Pipeline(NodePipelineError),
    Storage(StorageBookError),
    P2p(P2pError),
    MissingCapability(Capability),
    EmptyBatchSet,
}

impl fmt::Display for NodeRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(err) => write!(f, "config error: {err}"),
            Self::Pipeline(err) => write!(f, "pipeline error: {err}"),
            Self::Storage(err) => write!(f, "storage error: {err}"),
            Self::P2p(err) => write!(f, "p2p error: {err}"),
            Self::MissingCapability(capability) => {
                write!(f, "node missing required capability {capability:?}")
            }
            Self::EmptyBatchSet => write!(f, "cannot build epoch commit from an empty batch set"),
        }
    }
}

impl std::error::Error for NodeRuntimeError {}

impl From<NodeConfigError> for NodeRuntimeError {
    fn from(value: NodeConfigError) -> Self {
        Self::Config(value)
    }
}

impl From<NodePipelineError> for NodeRuntimeError {
    fn from(value: NodePipelineError) -> Self {
        Self::Pipeline(value)
    }
}

impl From<StorageBookError> for NodeRuntimeError {
    fn from(value: StorageBookError) -> Self {
        Self::Storage(value)
    }
}

impl From<P2pError> for NodeRuntimeError {
    fn from(value: P2pError) -> Self {
        Self::P2p(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalNodeRuntime {
    pub config: NodeConfig,
    pub retention_book: LocalRetentionBook,
}

impl LocalNodeRuntime {
    pub fn new(config: NodeConfig, retention_book: LocalRetentionBook) -> Self {
        Self {
            config,
            retention_book,
        }
    }

    pub fn collect_batch_from_sample(
        &self,
        epoch_id: EpochId,
        slot_id: u64,
        sample: SteamCurrentPlayersSample,
    ) -> Result<AssembledBatch, NodeRuntimeError> {
        self.collect_batch_from_samples(epoch_id, slot_id, vec![sample])
    }

    pub fn collect_batch_from_samples(
        &self,
        epoch_id: EpochId,
        slot_id: u64,
        samples: Vec<SteamCurrentPlayersSample>,
    ) -> Result<AssembledBatch, NodeRuntimeError> {
        self.ensure_collect_enabled()?;
        let collector_id = self.config.node_id()?;
        let mut builder = BatchBuilder::new(epoch_id, collector_id);

        for sample in samples {
            let signature = development_signature_placeholder(
                epoch_id,
                slot_id,
                sample.app_id,
                sample.observed_players,
            );
            let observation =
                sample.into_observation(epoch_id, slot_id, collector_id, signature)?;
            builder.push(observation)?;
        }

        Ok(builder.finalize(0)?)
    }

    pub fn collect_and_store_samples(
        &mut self,
        epoch_id: EpochId,
        slot_id: u64,
        samples: Vec<SteamCurrentPlayersSample>,
    ) -> Result<CollectAndStoreOutcome, NodeRuntimeError> {
        let assembled_batch = self.collect_batch_from_samples(epoch_id, slot_id, samples)?;
        let node_id = self.config.node_id()?;

        let stored_payload = if self.config.capabilities.store {
            Some(self.retention_book.record_batch_payload(
                node_id,
                epoch_id,
                effective_min_retention_epochs(&self.config),
                &assembled_batch.payload_bytes,
            )?)
        } else {
            None
        };

        Ok(CollectAndStoreOutcome {
            assembled_batch,
            stored_payload,
            batch_recipients: 0,
            receipt_recipients: 0,
        })
    }

    pub fn collect_store_and_publish(
        &mut self,
        epoch_id: EpochId,
        slot_id: u64,
        sample: SteamCurrentPlayersSample,
        network: &mut (impl P2pNetwork + ?Sized),
    ) -> Result<CollectAndStoreOutcome, NodeRuntimeError> {
        self.collect_store_and_publish_samples(epoch_id, slot_id, vec![sample], network)
    }

    pub fn collect_store_and_publish_samples(
        &mut self,
        epoch_id: EpochId,
        slot_id: u64,
        samples: Vec<SteamCurrentPlayersSample>,
        network: &mut (impl P2pNetwork + ?Sized),
    ) -> Result<CollectAndStoreOutcome, NodeRuntimeError> {
        let assembled_batch = self.collect_batch_from_samples(epoch_id, slot_id, samples)?;
        let node_id = self.config.node_id()?;

        network.advertise_payload(
            node_id,
            assembled_batch.payload_cid.clone(),
            assembled_batch.payload_hash,
            assembled_batch.payload_bytes.clone(),
        )?;

        let batch_recipients = publish_best_effort(
            network,
            node_id,
            P2pMessage::Batch(batch_announcement_from_assembled(&assembled_batch)),
        )?;

        let stored_payload = if self.config.capabilities.store {
            Some(self.retention_book.record_batch_payload(
                node_id,
                epoch_id,
                effective_min_retention_epochs(&self.config),
                &assembled_batch.payload_bytes,
            )?)
        } else {
            None
        };

        let receipt_recipients = if let Some(record) = &stored_payload {
            publish_best_effort(
                network,
                node_id,
                P2pMessage::ReplicaReceipt(replica_receipt_announcement_from_record(record)),
            )?
        } else {
            0
        };

        Ok(CollectAndStoreOutcome {
            assembled_batch,
            stored_payload,
            batch_recipients,
            receipt_recipients,
        })
    }

    pub fn build_epoch_commit(
        &self,
        inputs: EpochCommitInputs<'_>,
    ) -> Result<EpochCommit, NodeRuntimeError> {
        self.ensure_propose_enabled()?;
        if inputs.batches.is_empty() {
            return Err(NodeRuntimeError::EmptyBatchSet);
        }

        let proposer_id = self.config.node_id()?;
        let accepted_batches_root = merkle_root(
            &inputs
                .batches
                .iter()
                .map(|batch| {
                    stable_hash32(
                        &borsh::to_vec(&batch.batch_commit).expect("batch commit encoding"),
                    )
                })
                .collect::<Vec<_>>(),
        );

        let observation_hashes = inputs
            .batches
            .iter()
            .flat_map(|batch| batch.observations.iter())
            .map(hash_observation)
            .collect::<Vec<_>>();
        let observations_root = merkle_root(&observation_hashes);

        let availability_root = merkle_root(
            &inputs
                .stored_payloads
                .iter()
                .map(|record| {
                    stable_hash32(&borsh::to_vec(record).expect("stored payload encoding"))
                })
                .collect::<Vec<_>>(),
        );

        let randomness_seed = stable_hash32(
            &borsh::to_vec(&(inputs.epoch_id, accepted_batches_root, observations_root))
                .expect("seed encoding"),
        );

        Ok(EpochCommit {
            epoch_id: inputs.epoch_id,
            accepted_batches: MerkleCommitment {
                root: accepted_batches_root,
                leaf_count: inputs.batches.len() as u32,
            },
            observations: MerkleCommitment {
                root: observations_root,
                leaf_count: observation_hashes.len() as u32,
            },
            aggregates: MerkleCommitment {
                root: inputs.aggregates_root,
                leaf_count: u32::from(inputs.aggregates_root != [0u8; 32]),
            },
            rewards: MerkleCommitment {
                root: inputs.rewards_root,
                leaf_count: u32::from(inputs.rewards_root != [0u8; 32]),
            },
            availability: MerkleCommitment {
                root: availability_root,
                leaf_count: inputs.stored_payloads.len() as u32,
            },
            randomness_seed,
            proposer_id,
            challenge_open_height: inputs.current_height,
            challenge_deadline_height: inputs.current_height
                + inputs.challenge_window_blocks as u64,
        })
    }

    fn ensure_collect_enabled(&self) -> Result<(), NodeRuntimeError> {
        if !self.config.capabilities.collect || !self.config.collect.enabled {
            return Err(NodeRuntimeError::MissingCapability(Capability::Collect));
        }
        Ok(())
    }

    fn ensure_propose_enabled(&self) -> Result<(), NodeRuntimeError> {
        if !self.config.capabilities.propose {
            return Err(NodeRuntimeError::MissingCapability(Capability::Propose));
        }
        Ok(())
    }
}

fn publish_best_effort(
    network: &mut (impl P2pNetwork + ?Sized),
    from: NodeId,
    message: P2pMessage,
) -> Result<usize, NodeRuntimeError> {
    match network.publish(from, message) {
        Ok(count) => Ok(count),
        Err(P2pError::NoSubscribers(_)) => Ok(0),
        Err(err) => Err(NodeRuntimeError::P2p(err)),
    }
}

fn hash_observation(observation: &ObservationRecord) -> Hash32 {
    stable_hash32(&borsh::to_vec(observation).expect("observation encoding"))
}

fn development_signature_placeholder(
    epoch_id: EpochId,
    slot_id: u64,
    app_id: u32,
    observed_players: u64,
) -> Vec<u8> {
    let seed = format!("dev-sig:{epoch_id}:{slot_id}:{app_id}:{observed_players}");
    stable_hash32(seed.as_bytes()).to_vec()
}
