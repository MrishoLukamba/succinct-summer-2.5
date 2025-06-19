#[tokio::main]
async fn main() {
    // Load environment variables from .env file
    dotenv::dotenv().ok();

    println!("Hello, world!");
}

// ================================ ORCHESTRATOR ================================

// STORAGE LAYOUT ( we are using redis )
// 1. provers: Map<String, ProverProfile>
// 2. contests: Vec<Contest>
// temp storage
// 1. current_contest: Contest

mod execution;
mod networking;

use crate::execution::{VerifierExecutor, VerifierExecutorImpl};
use crate::networking::{ProverNetwork, ProverNetworkRpcServer};
pub use jsonrpsee::server::ServerBuilder;
use log::info;
use primitives::data_structure::{
    BidRequest, BidResponse, BidStatus, Contest, ProofData,ProverProfile, ContestStatus,ProofStatus, CREDIT_SLASH, PROOF_DURATION,
};
use redis::Client as RedisClient;
use std::env;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};
use anyhow::anyhow;
use redis::ConnectionLike;
use redis::Commands;

pub struct MainOrchestrator {
    pub redis_client: RedisClient,
    pub rpc_interface: ProverNetwork,
    pub execution_interface: VerifierExecutorImpl,
    pub bid_receiver_channel: Arc<Mutex<Receiver<BidRequest>>>,
    pub contest_sender_channel: Arc<Mutex<Sender<Contest>>>,
    pub proof_receiver_channel: Arc<Mutex<Receiver<ProofData>>>,
    pub proof_status_sender_channel: Arc<Mutex<Sender<ProofData>>>,
}

impl MainOrchestrator {
    pub fn new() -> Result<Self, anyhow::Error> {
        // Get Redis URL from environment variable
        let redis_url =
            env::var("REDIS_URL").map_err(|e| anyhow::anyhow!("Failed to get REDIS_URL: {}", e))?;
        let redis_client = RedisClient::open(redis_url)?;

        let (bid_sender_channel, bid_receiver_channel) = tokio::sync::mpsc::channel(10);
        let (contest_sender_channel, contest_receiver_channel) = tokio::sync::mpsc::channel(10);
        let (proof_sender_channel, proof_receiver_channel) = tokio::sync::mpsc::channel(10);
        let (proof_status_sender_channel, proof_status_receiver_channel) =
            tokio::sync::mpsc::channel(10);

        let execution_interface = VerifierExecutorImpl::new();

        let rpc_interface = ProverNetwork::new(
            redis_client.clone(),
            Contest::default(),
            proof_sender_channel,
            bid_sender_channel,
            contest_receiver_channel,
            proof_status_receiver_channel,
        );

        Ok(Self {
            redis_client,
            rpc_interface,
            execution_interface,
            bid_receiver_channel: Arc::new(Mutex::new(bid_receiver_channel)),
            contest_sender_channel: Arc::new(Mutex::new(contest_sender_channel)),
            proof_receiver_channel: Arc::new(Mutex::new(proof_receiver_channel)),
            proof_status_sender_channel: Arc::new(Mutex::new(proof_status_sender_channel)),
        })
    }

    pub async fn listen_and_process_bids(&self) {
        let mut bid_receiver = self.bid_receiver_channel.lock().await;
        while let Some(mut bid) = bid_receiver.recv().await {
            // check if the contest is still running;
            let current_contest = self.execution_interface.current_contest.clone();
            if current_contest.is_live() {
                self.execution_interface.add_bid(bid);
            } else {
                bid.bid_status = BidStatus::Rejected;
                // Store rejected bid in Redis
                if !self.redis_client.is_open() {
                    self.redis_client.get_connection().expect("Failed to get connection");
                }
                let prover_profile = self.redis_client.clone().hget::<String, String, String>("provers".to_string(), bid.prover_address.clone()).expect("Failed to get prover profile");
                let mut prover_profile = serde_json::from_str::<ProverProfile>(&prover_profile).expect("Failed to deserialize prover profile");
                prover_profile.bids.push(bid.clone());
                self.redis_client.clone().hset::<String, String, String, String>("provers".to_string(), bid.prover_address.clone(), serde_json::to_string(&prover_profile).unwrap()).expect("Failed to store prover profile");
            }
        }
    }

    pub async fn start_new_contest(&self) {
        // always check the status of the current contest and if it is ended, wait for the proof duration to start a new contest
        loop {
            let contest = self.execution_interface.current_contest.clone();
            if contest.status == ContestStatus::Ended {
                // wait for the proof duration to start a new contest
                sleep(Duration::from_secs(PROOF_DURATION)).await;
                self.execution_interface.start_contest();
            }
        }
    }

    pub async fn process_contest_completion(&self) {
        // Poll for winner until one is found
        loop {
            let contest = self.execution_interface.current_contest.clone();
            if contest.status == ContestStatus::Ended {
                // get the winner
                let winner = contest.calculate_winner();
                if let Some(winner) = winner {
                    // store the winner in redis
                    self.store_contest_in_redis(&contest).await;
                    info!("Winner found: {:?}", winner.prover_name);
                    self.contest_sender_channel
                        .lock()
                        .await
                        .send(contest)
                        .await
                        .expect("Failed to send contest");
                }
            }
        }
    }

    pub async fn store_contest_in_redis(&self, contest: &Contest) {
        // Store the completed contest in Redis
        if !self.redis_client.is_open() {
            self.redis_client
                .get_connection()
                .expect("Failed to get connection");
        }
        self.redis_client
            .clone()
            .rpush::<String, String, String>(
                "contest".to_string(),
                serde_json::to_string(contest).unwrap(),
            )
            .expect("Failed to store contest in Redis");
    }

    pub async fn process_proof(&self) {
        let mut proof_receiver = self.proof_receiver_channel.lock().await;
        loop {
            while let Some(mut proof_data) = proof_receiver.recv().await {
                // check if its within the proving window
                let current_contest = self.execution_interface.current_contest.clone();
                if current_contest.is_live()
                    || current_contest.end_time + PROOF_DURATION
                        < proof_data.proof_header.proof_timestamp
                {
                    // reject the proof
                    proof_data.proof_header.proof_status = ProofStatus::Rejected;
                    // deduct credit from the prover
                    if !self.redis_client.is_open() {
                        self.redis_client.get_connection().expect("Failed to get connection");
                    }
                    let prover_profile = self.redis_client.clone().hget::<String, String, String>("provers".to_string(), proof_data.proof_header.prover_name.clone()).expect("Failed to get prover profile");
                    let mut prover_profile = serde_json::from_str::<ProverProfile>(&prover_profile).expect("Failed to deserialize prover profile");
                    
                    prover_profile.prover_credits -= CREDIT_SLASH;
                    self.redis_client.clone().hset::<String, String, String, String>("provers".to_string(), proof_data.proof_header.prover_name.clone(), serde_json::to_string(&prover_profile).unwrap()).expect("Failed to store prover profile");
                    
                    self.proof_status_sender_channel
                        .lock()
                        .await
                        .send(proof_data.clone())
                        .await
                        .expect("Failed to send proof status");
                    info!("Proof rejected: {:?}", proof_data.proof_header.prover_name);
                }

                match self
                    .execution_interface
                    .verify_proof(proof_data.clone())
                    
                {
                    Ok(_) => {
                        proof_data.proof_header.proof_status = ProofStatus::Accepted;
                        // add credit to the prover
                        if !self.redis_client.is_open() {
                            self.redis_client.get_connection().expect("Failed to get connection");
                        }
                        let prover_profile = self.redis_client.clone().hget::<String, String, String>("provers".to_string(), proof_data.proof_header.prover_name.clone()).expect("Failed to get prover profile");
                        let mut prover_profile = serde_json::from_str::<ProverProfile>(&prover_profile).expect("Failed to deserialize prover profile");
                        prover_profile.prover_credits += current_contest.reward;
                        self.redis_client.clone().hset::<String, String, String, String>("provers".to_string(), proof_data.proof_header.prover_name.clone(), serde_json::to_string(&prover_profile).unwrap()).expect("Failed to store prover profile");
                        
                        self.proof_status_sender_channel
                            .lock()
                            .await
                            .send(proof_data.clone())
                            .await
                            .expect("Failed to send proof status");
                        info!("Proof accepted: {:?}", proof_data.proof_header.prover_name);
                    }
                    Err(e) => {
                        info!("Proof verification failed: {}", e);
                    }
                }
            }
        }
    }

    pub async fn start_rpc_server(&self, rpc_port: u16) -> Result<(), anyhow::Error> {
        let server_builder = ServerBuilder::new();

        let url = format!("127.0.0.1:{}", rpc_port);

        let rpc_handler = self.rpc_interface.clone();

        let server = server_builder.build(url).await?;
        let address = server
            .local_addr()
            .expect("failed to get address");
        let handle = server
            .start(rpc_handler.into_rpc());

        tokio::spawn(handle.stopped());
        info!("RPC server started on {}", address);
        Ok(())
    }

    pub async fn start(&self, rpc_port: u16) {
        // start the rpc server
        self.start_rpc_server(rpc_port).await;

        let bid_processor = self.listen_and_process_bids();
        let contest_manager = self.start_new_contest();
        let proof_processor = self.process_proof();
        let winner_poller = self.process_contest_completion();

        tokio::join!(
            bid_processor,
            contest_manager,
            proof_processor,
            winner_poller
        );
    }
}
