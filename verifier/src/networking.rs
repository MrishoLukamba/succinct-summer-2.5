use jsonrpsee::{
    core::{async_trait, RpcResult, SubscriptionResult},
    proc_macros::rpc,
    types::ErrorObject,
    PendingSubscriptionSink, SubscriptionMessage,
};
use log::info;
use primitives::data_structure::{
    BidRequest, BidResponse, Contest, ProofData, ProverProfile, Team, ETH_BLOCK_PROGRAM,
    ETH_TXN_INPUT,
};
use redis::Commands;
use redis::{Client as RedisClient, ConnectionLike};
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Mutex;

use crate::StorageLayer;

#[rpc(server, client)]
pub trait ProverNetworkRpc {
    /// Submit proof data to be sent to the verifier
    /// params:
    /// - proofData: The proof data to be sent to the verifier
    #[method(name = "submit_proof")]
    async fn submit_proof(&self, proof_data: ProofData) -> RpcResult<()>;

    /// Submit bid request to the verifier
    /// params:
    /// - bid_request: The bid request to be sent to the verifier
    #[subscription(name = "submit_bid", unsubscribe = "unsub_submit_bid", item = Option<BidResponse>)]
    async fn submit_bid(&self, bid_request: BidRequest) -> SubscriptionResult;

    /// Register a new prover
    /// params:
    /// - prover_name: The name of the prover
    /// - prover_team: The team of the prover
    #[method(name = "register_prover")]
    async fn register_prover(
        &self,
        prover_name: String,
        prover_team: Team,
        prover_address: String,
    ) -> RpcResult<()>;

    /// ======================= GETTERS =======================
    /// Watch the current contest
    #[subscription(name = "watch_current_contest", unsubscribe = "unsub_watch_current_contest", item = Contest)]
    async fn watch_current_contest(&self) -> SubscriptionResult;

    /// Watch the current proof execution
    #[subscription(name = "watch_proof_status", unsubscribe = "unsub_watch_proof_status", item = ProofData)]
    async fn watch_proof_status(&self) -> SubscriptionResult;

    /// Get the current provers
    #[method(name = "get_provers")]
    async fn get_provers(&self) -> RpcResult<Vec<ProverProfile>>;

    /// Get all contests
    #[method(name = "get_all_contests")]
    async fn get_all_contests(&self) -> RpcResult<Vec<Contest>>;
}

#[derive(Clone)]
pub struct ProverNetwork {
    pub redis_service: StorageLayer,
    pub current_contest: Arc<Mutex<Contest>>,
    pub proof_sender_channel: Arc<Mutex<Sender<ProofData>>>,
    pub bid_sender_channel: Arc<Mutex<Sender<BidRequest>>>,
    pub contest_response_receiver_channel: Arc<Mutex<Receiver<Contest>>>,
    pub proof_status_receiver_channel: Arc<Mutex<Receiver<ProofData>>>,
}

impl ProverNetwork {
    pub fn new(
        redis_service: StorageLayer,
        current_contest: Contest,
        proof_sender_channel: Sender<ProofData>,
        bid_sender_channel: Sender<BidRequest>,
        contest_response_receiver_channel: Receiver<Contest>,
        proof_status_receiver_channel: Receiver<ProofData>,
    ) -> Self {
        Self {
            redis_service,
            current_contest: Arc::new(Mutex::new(current_contest)),
            proof_sender_channel: Arc::new(Mutex::new(proof_sender_channel)),
            bid_sender_channel: Arc::new(Mutex::new(bid_sender_channel)),
            contest_response_receiver_channel: Arc::new(Mutex::new(
                contest_response_receiver_channel,
            )),
            proof_status_receiver_channel: Arc::new(Mutex::new(proof_status_receiver_channel)),
        }
    }
}

#[async_trait]
impl ProverNetworkRpcServer for ProverNetwork {
    async fn submit_proof(&self, proof_data: ProofData) -> RpcResult<()> {
        proof_data
            .sanity_check()
            .map_err(|e| ErrorObject::owned(1, format!("Invalid proof data: {}", e), None::<()>))?;
        self.proof_sender_channel
            .lock()
            .await
            .send(proof_data.clone())
            .await
            .map_err(|e| {
                ErrorObject::owned(
                    2,
                    format!("Failed to send proof data to prover: {}", e),
                    None::<()>,
                )
            })?;
        info!(
            "Proof submitted successfully: {:?}, by {:?}",
            proof_data.proof_header.proof_timestamp, proof_data.proof_header.prover_address
        );

        let mut prover_profile = self.redis_service.get_prover_profile(&proof_data.proof_header.prover_name)
            .map_err(|e| {
                ErrorObject::owned(
                    3,
                    format!("Failed to get prover profile: {}", e),
                    None::<()>,
                )
            })?;
        prover_profile.proofs.push(proof_data.clone());
        self.redis_service.update_prover_profile(&prover_profile)
            .map_err(|e| {
                ErrorObject::owned(5, format!("Failed to store proof data: {}", e), None::<()>)
            })?;
        info!(
            "Proof stored successfully: {:?}, by {:?}",
            proof_data.proof_header.proof_timestamp, proof_data.proof_header.prover_name
        );

        Ok(())
    }

    async fn submit_bid(
        &self,
        pending_subscription: PendingSubscriptionSink,
        bid_request: BidRequest,
    ) -> SubscriptionResult {
        let sink = pending_subscription.accept().await?;

        self.bid_sender_channel
            .lock()
            .await
            .send(bid_request.clone())
            .await
            .map_err(|e| {
                ErrorObject::owned(
                    3,
                    format!("Failed to send bid request to prover: {}", e),
                    None::<()>,
                )
            })?;
        info!(
            "Bid submitted successfully: {:?}, by {:?}",
            bid_request.bid_amount, bid_request.prover_address
        );
        // wait for contest to finish and return if bidResponse if winner
        let mut contest_response_receiver = self.contest_response_receiver_channel.lock().await;
        let time_now = std::time::Instant::now().elapsed().as_secs();
        let current_contest_time = self.current_contest.lock().await.end_time + 5;

        let result = tokio::time::timeout(
            tokio::time::Duration::from_secs(current_contest_time - time_now),
            async move {
                while let Some(contest) = contest_response_receiver.recv().await {
                    match contest.winner {
                        Some(winner) => {
                            if winner.prover_address == bid_request.prover_address {
                                info!(
                                    "Bid won: {:?}, by {:?}",
                                    bid_request.bid_amount, bid_request.prover_address
                                );
                                let bid_response = BidResponse {
                                    program: ETH_BLOCK_PROGRAM.to_vec(),
                                    input: ETH_TXN_INPUT.to_vec(),
                                    contest_id: contest.contest_id,
                                };
                                return Some(bid_response);
                            }
                        }
                        None => {
                            continue;
                        }
                    }
                }
                None
            },
        )
        .await?;

        let json_value =
            SubscriptionMessage::new(sink.method_name(), sink.subscription_id(), &result).unwrap();

        sink.send(json_value).await?;
        info!("Generate proof sent to prover: {:?}", result);
        Ok(())
    }

    async fn register_prover(
        &self,
        prover_name: String,
        prover_team: Team,
        prover_address: String,
    ) -> RpcResult<()> {
        let prover_profile = ProverProfile::new(prover_name.clone(), prover_team, prover_address);
        self.redis_service.store_prover_profile(&prover_profile)
            .map_err(|e| {
                ErrorObject::owned(6, format!("Failed to register prover: {}", e), None::<()>)
            })?;
        info!(
            "Prover registered successfully: {:?}",
            prover_profile.prover_name
        );
        Ok(())
    }

    async fn watch_current_contest(
        &self,
        pending_subscription: PendingSubscriptionSink,
    ) -> SubscriptionResult {
        let sink = pending_subscription.accept().await?;

        let current_contest = {
            let contest_guard = self.current_contest.lock().await;
            contest_guard.clone()
        };

        let json_value =
            SubscriptionMessage::new(sink.method_name(), sink.subscription_id(), &current_contest)
                .unwrap();
        sink.send(json_value).await?;
        Ok(())
    }

    async fn get_provers(&self) -> RpcResult<Vec<ProverProfile>> {
        let provers = self.redis_service.get_all_provers()
            .map_err(|e| {
                ErrorObject::owned(5, format!("Failed to get provers: {}", e), None::<()>)
            })?;
        info!("Provers fetched successfully: {}", provers.len());
        Ok(provers)
    }

    async fn get_all_contests(&self) -> RpcResult<Vec<Contest>> {
        let contests = self.redis_service.get_all_contests()
            .map_err(|e| {
                ErrorObject::owned(5, format!("Failed to get contests: {}", e), None::<()>)
            })?;
        info!("Contests fetched successfully");
        Ok(contests)
    }

    async fn watch_proof_status(
        &self,
        pending_subscription: PendingSubscriptionSink,
    ) -> SubscriptionResult {
        let sink = pending_subscription.accept().await?;
        let mut proof_status_receiver = self.proof_status_receiver_channel.lock().await;
        loop {
            while let Some(proof_data) = proof_status_receiver.recv().await {
                let json_value = SubscriptionMessage::new(
                    sink.method_name(),
                    sink.subscription_id(),
                    &proof_data,
                )
                .unwrap();
                sink.send(json_value).await?;
            }
        }
        Ok(())
    }
}
