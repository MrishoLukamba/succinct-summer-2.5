use clap::Parser;
use log::LevelFilter;
use simplelog::*;
use std::fs::File;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::sync::{Arc as StdArc, Mutex as StdMutex};

fn log_setup() -> Result<(), anyhow::Error> {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create("succinct-log.log").unwrap(),
        ),
    ])
    .unwrap();
    Ok(())
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    pub port: u16,
    #[arg(short, long, default_value = "memory")]
    pub storage: String,
}

#[tokio::main]
async fn main() {
    log_setup().unwrap();
    // Load environment variables from .env file
    dotenv::dotenv().ok();
    let args = Args::parse();
    
    // Convert storage string to StorageType enum
    let storage_type = match args.storage.to_lowercase().as_str() {
        "redis" => StorageType::Redis,
        "memory" => StorageType::InMemory,
        _ => {
            eprintln!("❌ Invalid storage type: {}. Use 'memory' or 'redis'", args.storage);
            std::process::exit(1);
        }
    };
    
    let orchestrator = MainOrchestrator::new(storage_type).unwrap();
    if let Err(e) = orchestrator.start(args.port).await {
        log::error!("Verifier failed to start: {}", e);
        std::process::exit(1);
    }
    info!("Verifier is running");
}

// ================================ STORAGE TRAIT ================================

#[derive(Debug, Clone)]
pub enum StorageType {
    Redis,
    InMemory,
}

pub trait Storage: Send + Sync {
    fn store_prover_profile(&self, profile: &ProverProfile) -> Result<(), anyhow::Error>;
    fn get_prover_profile(&self, prover_name: &str) -> Result<ProverProfile, anyhow::Error>;
    fn update_prover_profile(&self, profile: &ProverProfile) -> Result<(), anyhow::Error>;
    fn store_contest(&self, contest: &Contest) -> Result<(), anyhow::Error>;
    fn get_contest_count(&self) -> Result<u64, anyhow::Error>;
    fn get_all_provers(&self) -> Result<Vec<ProverProfile>, anyhow::Error>;
    fn get_all_contests(&self) -> Result<Vec<Contest>, anyhow::Error>;
    fn is_connected(&self) -> bool;
}

// ================================ IN-MEMORY STORAGE ================================

struct InMemoryStorageInner {
    prover_profiles: HashMap<String, String>,
    contests: Vec<String>,
}

pub struct InMemoryStorage {
    inner: StdArc<StdMutex<InMemoryStorageInner>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            inner: StdArc::new(StdMutex::new(InMemoryStorageInner {
                prover_profiles: HashMap::new(),
                contests: Vec::new(),
            })),
        }
    }
}

impl Storage for InMemoryStorage {
    fn store_prover_profile(&self, profile: &ProverProfile) -> Result<(), anyhow::Error> {
        let mut inner = self.inner.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock storage: {}", e))?;
        let serialized_profile = serde_json::to_string(profile)?;
        inner.prover_profiles.insert(profile.prover_name.clone(), serialized_profile);
        Ok(())
    }

    fn get_prover_profile(&self, prover_name: &str) -> Result<ProverProfile, anyhow::Error> {
        let inner = self.inner.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock storage: {}", e))?;
        
        let profile_json = inner.prover_profiles
            .get(prover_name)
            .ok_or_else(|| anyhow::anyhow!("No prover profile found for: {}", prover_name))?;
        
        let profile = serde_json::from_str::<ProverProfile>(profile_json)?;
        Ok(profile)
    }

    fn update_prover_profile(&self, profile: &ProverProfile) -> Result<(), anyhow::Error> {
        self.store_prover_profile(profile)
    }

    fn store_contest(&self, contest: &Contest) -> Result<(), anyhow::Error> {
        let mut inner = self.inner.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock storage: {}", e))?;
        let serialized_contest = serde_json::to_string(contest)?;
        inner.contests.push(serialized_contest);
        Ok(())
    }

    fn get_contest_count(&self) -> Result<u64, anyhow::Error> {
        let inner = self.inner.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock storage: {}", e))?;
        Ok(inner.contests.len() as u64)
    }

    fn get_all_provers(&self) -> Result<Vec<ProverProfile>, anyhow::Error> {
        let inner = self.inner.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock storage: {}", e))?;
        
        let provers = inner.prover_profiles
            .values()
            .map(|value| {
                serde_json::from_str::<ProverProfile>(value)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize prover: {}", e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(provers)
    }

    fn get_all_contests(&self) -> Result<Vec<Contest>, anyhow::Error> {
        let inner = self.inner.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock storage: {}", e))?;
        
        let contests_data = inner.contests
            .iter()
            .map(|value| {
                serde_json::from_str::<Contest>(value)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize contest: {}", e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(contests_data)
    }

    fn is_connected(&self) -> bool {
        true // In-memory storage is always "connected"
    }
}

// ================================ REDIS STORAGE ================================

pub struct RedisStorage {
    client: RedisClient,
}

impl RedisStorage {
    pub fn new() -> Result<Self, anyhow::Error> {
        let redis_url = env::var("REDIS_URL")
            .map_err(|e| anyhow::anyhow!("Failed to get REDIS_URL: {}", e))?;
        let client = RedisClient::open(redis_url)?;
        Ok(Self { client })
    }

    fn get_connection(&self) -> Result<redis::Connection, anyhow::Error> {
        self.client
            .get_connection()
            .map_err(|e| anyhow::anyhow!("Failed to connect to Redis: {}", e))
    }
}

impl Storage for RedisStorage {
    fn store_prover_profile(&self, profile: &ProverProfile) -> Result<(), anyhow::Error> {
        let mut conn = self.get_connection()?;
        let _ = conn.hset::<String, String, String, String>(
            "provers".to_string(),
            profile.prover_name.clone(),
            serde_json::to_string(profile)?,
        )
        .map_err(|e| anyhow::anyhow!("Failed to store prover profile: {}", e))?;
        Ok(())
    }

    fn get_prover_profile(&self, prover_name: &str) -> Result<ProverProfile, anyhow::Error> {
        let mut conn = self.get_connection()?;
        let profile_json: String = conn
            .hget("provers".to_string(), prover_name.to_string())
            .map_err(|e| anyhow::anyhow!("Failed to get prover profile: {}", e))?;
        
        let profile = serde_json::from_str(&profile_json)
            .map_err(|e| anyhow::anyhow!("Failed to deserialize prover profile: {}", e))?;
        Ok(profile)
    }

    fn update_prover_profile(&self, profile: &ProverProfile) -> Result<(), anyhow::Error> {
        self.store_prover_profile(profile)
    }

    fn store_contest(&self, contest: &Contest) -> Result<(), anyhow::Error> {
        let mut conn = self.get_connection()?;
        conn.rpush::<String, String, String>(
            "contests".to_string(),
            serde_json::to_string(contest)?,
        )
        .map_err(|e| anyhow::anyhow!("Failed to store contest: {}", e))?;
        Ok(())
    }

    fn get_contest_count(&self) -> Result<u64, anyhow::Error> {
        let mut conn = self.get_connection()?;
        let count = conn.llen("contests".to_string())
            .map_err(|e| anyhow::anyhow!("Failed to get contest count: {}", e))?;
        Ok(count)
    }

    fn get_all_provers(&self) -> Result<Vec<ProverProfile>, anyhow::Error> {
        let mut conn = self.get_connection()?;
        let provers_data: Vec<(String, String)> = conn
            .hgetall("provers".to_string())
            .map_err(|e| anyhow::anyhow!("Failed to get all provers: {}", e))?;
        
        let provers = provers_data
            .into_iter()
            .map(|(_, value)| {
                serde_json::from_str::<ProverProfile>(&value)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize prover: {}", e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(provers)
    }

    fn get_all_contests(&self) -> Result<Vec<Contest>, anyhow::Error> {
        let mut conn = self.get_connection()?;
        let contests_data: Vec<String> = conn
            .lrange("contests".to_string(), 0, -1)
            .map_err(|e| anyhow::anyhow!("Failed to get all contests: {}", e))?;
        
        let contests = contests_data
            .into_iter()
            .map(|value| {
                serde_json::from_str::<Contest>(&value)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize contest: {}", e))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(contests)
    }

    fn is_connected(&self) -> bool {
        self.client.is_open()
    }
}

mod execution;
mod networking;

use crate::execution::{VerifierExecutor, VerifierExecutorImpl};
use crate::networking::{ProverNetwork, ProverNetworkRpcServer};
use anyhow::anyhow;
pub use jsonrpsee::server::ServerBuilder;
use log::info;
use primitives::data_structure::{
    BidRequest, BidResponse, BidStatus, Contest, ContestStatus, ProofData, ProofStatus,
    ProverProfile, CONTEST_DURATION, CREDIT_SLASH, PROOF_DURATION,
};
use redis::Client as RedisClient;
use redis::Commands;
use redis::ConnectionLike;
use std::env;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::time::{sleep, Duration};

// ================================ ORCHESTRATOR ================================

pub struct MainOrchestrator {
    pub storage: Box<dyn Storage>,
    pub rpc_interface: ProverNetwork,
    pub execution_interface: VerifierExecutorImpl,
    pub bid_receiver_channel: Arc<Mutex<Receiver<BidRequest>>>,
    pub contest_sender_channel: Arc<Mutex<Sender<Contest>>>,
    pub proof_receiver_channel: Arc<Mutex<Receiver<ProofData>>>,
    pub proof_status_sender_channel: Arc<Mutex<Sender<ProofData>>>,
}

impl MainOrchestrator {
    pub fn new(storage_type: StorageType) -> Result<Self, anyhow::Error> {
        let storage: Box<dyn Storage> = match storage_type {
            StorageType::Redis => Box::new(RedisStorage::new()?),
            StorageType::InMemory => Box::new(InMemoryStorage::new()),
        };

        let (bid_sender_channel, bid_receiver_channel) = tokio::sync::mpsc::channel(10);
        let (contest_sender_channel, contest_receiver_channel) = tokio::sync::mpsc::channel(10);
        let (proof_sender_channel, proof_receiver_channel) = tokio::sync::mpsc::channel(10);
        let (proof_status_sender_channel, proof_status_receiver_channel) =
            tokio::sync::mpsc::channel(10);

        let execution_interface = VerifierExecutorImpl::new();

        // Create a separate storage instance for RPC interface
        let rpc_storage: Box<dyn Storage> = match storage_type {
            StorageType::Redis => Box::new(RedisStorage::new()?),
            StorageType::InMemory => Box::new(InMemoryStorage::new()),
        };

        let rpc_interface = ProverNetwork::new(
            rpc_storage,
            Contest::default(),
            proof_sender_channel,
            bid_sender_channel,
            contest_receiver_channel,
            proof_status_receiver_channel,
        );

        Ok(Self {
            storage,
            rpc_interface,
            execution_interface,
            bid_receiver_channel: Arc::new(Mutex::new(bid_receiver_channel)),
            contest_sender_channel: Arc::new(Mutex::new(contest_sender_channel)),
            proof_receiver_channel: Arc::new(Mutex::new(proof_receiver_channel)),
            proof_status_sender_channel: Arc::new(Mutex::new(proof_status_sender_channel)),
        })
    }

    pub async fn listen_and_process_bids(&self) -> Result<(), anyhow::Error> {
        let mut bid_receiver = self.bid_receiver_channel.lock().await;
        while let Some(mut bid) = bid_receiver.recv().await {
            info!(
                "Received bid: {:?} from {:?}",
                bid.bid_amount, bid.prover_name
            );
            // check if the contest is still running;
            let current_contest = self.execution_interface.current_contest.clone();
            if current_contest.is_live() {
                let is_valid = self.execution_interface.clone().add_bid(bid.clone());
                if is_valid {
                    info!(
                        "Bid accepted: {:?} from {:?}",
                        bid.bid_amount, &bid.prover_name
                    );
                } else {
                    info!(
                        "Bid rejected: {:?} from {:?}",
                        bid.bid_amount, &bid.prover_name
                    );
                }
            } else {
                bid.bid_status = BidStatus::Rejected;
                // Store rejected bid in Redis
                let mut prover_profile = self.storage.get_prover_profile(&bid.prover_address)?;
                prover_profile.bids.push(bid.clone());
                self.storage.update_prover_profile(&prover_profile)?;
                info!(
                    "Bid rejected: {:?} from {:?}",
                    bid.bid_amount, bid.prover_name
                );
            }
        }
        Ok(())
    }

    pub async fn start_new_contest(&self) -> Result<(), anyhow::Error> {
        // always check the status of the current contest and if it is ended, wait for the proof duration to start a new contest
        loop {
            let contest = self.execution_interface.current_contest.clone();
            match contest.status {
                ContestStatus::Live => {
                    // wait for the contest duration to start a new contest
                    sleep(Duration::from_secs(CONTEST_DURATION)).await;
                    self.execution_interface.clone().end_contest();
                    info!("Contest {} ended", contest.contest_id);
                }
                ContestStatus::Ended => {
                    self.storage.store_contest(&contest)?;
                    // wait for the proof duration to start a new contest
                    sleep(Duration::from_secs(PROOF_DURATION)).await;
                    let next_id = self.storage.get_contest_count()?;
                    self.execution_interface.clone().start_contest(next_id);
                    info!("New contest {} started after previous contest ended", next_id);
                }
                ContestStatus::NotStarted => {
                    let next_id = self.storage.get_contest_count()?;
                    self.execution_interface.clone().start_contest(next_id);
                }
            }
        }
    }

    pub async fn process_contest_completion(&self) -> Result<(), anyhow::Error> {
        // Poll for winner until one is found
        loop {
            let contest = self.execution_interface.current_contest.clone();
            if contest.status == ContestStatus::Ended {
                // get the winner
                let mut execution_interface = self.execution_interface.clone();
                let winner = execution_interface.get_winner();
                if let Some(winner) = winner {
                    // store the winner in redis
                    self.store_contest_in_redis(&contest).await;
                    info!("Winner found: {:?}", winner.prover_name);
                    self.contest_sender_channel
                        .lock()
                        .await
                        .send(contest)
                        .await?;
                }
            }
        }
    }

    pub async fn store_contest_in_redis(&self, contest: &Contest) {
        // Store the completed contest in Redis
        if let Err(e) = self.storage.store_contest(contest) {
            log::error!("Failed to store contest in Redis: {}", e);
        }
    }

    pub async fn process_proof(&self) -> Result<(), anyhow::Error> {
        let mut proof_receiver = self.proof_receiver_channel.lock().await;
        loop {
            while let Some(mut proof_data) = proof_receiver.recv().await {
                info!(
                    "Received proof: {:?} from {:?}",
                    proof_data.proof, proof_data.proof_header.prover_name
                );
                // check if its within the proving window
                let current_contest = self.execution_interface.current_contest.clone();
                if current_contest.is_live()
                    || current_contest.end_time + PROOF_DURATION
                        < proof_data.proof_header.proof_timestamp
                {
                    // reject the proof
                    proof_data.proof_header.proof_status = ProofStatus::Rejected;
                    // deduct credit from the prover
                    let mut prover_profile = self.storage.get_prover_profile(&proof_data.proof_header.prover_name)?;
                    prover_profile.prover_credits -= CREDIT_SLASH;
                    self.storage.update_prover_profile(&prover_profile)?;

                    self.proof_status_sender_channel
                        .lock()
                        .await
                        .send(proof_data.clone())
                        .await
                        .expect("Failed to send proof status");
                    info!("Proof rejected: {:?}", proof_data.proof_header.prover_name);
                }

                match self.execution_interface.verify_proof(proof_data.clone()) {
                    Ok(_) => {
                        proof_data.proof_header.proof_status = ProofStatus::Accepted;
                        // add credit to the prover
                        let mut prover_profile = self.storage.get_prover_profile(&proof_data.proof_header.prover_name)?;
                        prover_profile.prover_credits += current_contest.reward;
                        self.storage.update_prover_profile(&prover_profile)?;

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
        Ok(())
    }

    pub async fn start_rpc_server(&self, rpc_port: u16) -> Result<(), anyhow::Error> {
        let server_builder = ServerBuilder::new();

        let url = format!("127.0.0.1:{}", rpc_port);

        let rpc_handler = self.rpc_interface.clone();

        let server = server_builder.build(url).await?;
        let address = server.local_addr().expect("failed to get address");
        let handle = server.start(rpc_handler.into_rpc());

        tokio::spawn(handle.stopped());
        info!("Succinct Verifier WebSocket RPC server started on {}", address);
        Ok(())
    }

    pub async fn start(&self, rpc_port: u16) -> Result<(), anyhow::Error> {
        // start the rpc server
        self.start_rpc_server(rpc_port).await?;

        let bid_processor = self.listen_and_process_bids();
        let contest_manager = self.start_new_contest();
        let proof_processor = self.process_proof();
        let winner_poller = self.process_contest_completion();

        // Handle the contest manager result
        let contest_result = contest_manager.await;
        if let Err(e) = contest_result {
            info!("Contest manager error: {}", e);
        }

        let _res = tokio::join!(
            bid_processor,
            proof_processor,
            winner_poller
        );
        
        Ok(())
    }
}
