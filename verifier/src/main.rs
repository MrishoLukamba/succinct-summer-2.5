use clap::Parser;
use log::LevelFilter;
use simplelog::*;
use core::task;
use std::fs::File;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::sync::{Arc as StdArc, Mutex as StdMutex};
use futures_util::FutureExt;
use log::error;
use tokio::sync::mpsc::{self, Sender, Receiver};
use tokio::time::{sleep, Duration};

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
    
    if let Err(e) = MainOrchestrator::start(storage_type, args.port).await {
        println!("\n\n");
        println!("    ███████╗██╗   ██╗ ██████╗ ██████╗██╗███╗   ██╗ ██████╗████████╗");
        println!("    ██╔════╝██║   ██║██╔════╝██╔════╝██║████╗  ██║██╔════╝╚══██╔══╝");
        println!("    ███████╗██║   ██║██║     ██║     ██║██╔██╗ ██║██║        ██║   ");
        println!("    ╚════██║██║   ██║██║     ██║     ██║██║╚██╗██║██║        ██║   ");
        println!("    ███████║╚██████╔╝╚██████╗╚██████╗██║██║ ╚████║╚██████╗   ██║   ");
        println!("    ╚══════╝ ╚═════╝  ╚═════╝ ╚═════╝╚═╝╚═╝  ╚═══╝ ╚═════╝   ╚═╝   ");
        println!();
        println!("         ██╗   ██╗███████╗██████╗ ██╗███████╗██╗███████╗██████╗    ");
        println!("         ██║   ██║██╔════╝██╔══██╗██║██╔════╝██║██╔════╝██╔══██╗   ");
        println!("         ██████╔╝██████╔╝██║   ██║██║█████╗  ██║█████╗  ██████╔╝   ");
        println!("         ╚██╗ ██╔╝██╔══╝  ██╔══██╗██║██╔══╝  ██║██╔══╝  ██╔══██╗   ");
        println!("          ╚████╔╝ ███████╗██║  ██║██║██║     ██║███████╗██║  ██║   ");
        println!("           ╚═══╝  ╚══════╝╚═╝  ╚═╝╚═╝╚═╝     ╚═╝╚══════╝╚═╝  ╚═╝   ");
        println!();
        println!("             ✅ Succinct Verifier Client CLI started! ✅ \n\n");
        std::process::exit(1);
    }
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
        info!("Getting prover profile for: {}", prover_name);
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
pub use jsonrpsee::server::{ServerBuilder,ServerConfigBuilder};
use log::info;
use primitives::data_structure::{
    BidRequest, BidResponse, BidStatus, Contest, ContestStatus, ProofData, ProofStatus,
    ProverProfile, CONTEST_DURATION, CREDIT_SLASH, PROOF_DURATION,
};
use redis::Client as RedisClient;
use redis::Commands;
use redis::ConnectionLike;
use std::env;

// ================================ ORCHESTRATOR ================================

#[derive(Clone)]
pub struct MainOrchestrator {
    pub storage: Arc<dyn Storage>,
    pub rpc_interface: ProverNetwork,
    pub execution_interface: Arc<Mutex<VerifierExecutorImpl>>,
}

// Channel message types for better type safety
#[derive(Debug, Clone)]
pub enum ContestCommand {
    StartContest(u64),
    EndContest,
    CheckStatus,
}

#[derive(Clone)]
pub enum ExecutionCommand {
    AddBid(BidRequest),
    GetWinner,
    VerifyProof(ProofData),
    GetContestStatus,
}

impl MainOrchestrator {
    pub async fn listen_and_process_bids(&self, mut bid_receiver: Receiver<BidRequest>) -> Result<(), anyhow::Error> {
        info!("🎯 listen_and_process_bids task started!");
        
        while let Some(mut bid) = bid_receiver.recv().await {
            info!(
                "📨 Received bid: {:?} from {:?}",
                bid.bid_amount, bid.prover_name
            );
            
            // Get contest status with minimal lock time
            let is_live = {
                let current_contest = self.execution_interface.lock().await.current_contest.clone();
                current_contest.is_live()
            };
            
            if is_live {
                // Add bid with minimal lock time
                let is_valid = {
                    let mut execution = self.execution_interface.lock().await;
                    execution.add_bid(bid.clone())
                };
                
                if is_valid {
                    info!("✅ Bid accepted");
                } else {
                    info!("❌ Bid rejected");
                }
            } else {
                info!("⏰ contest is not live");
                bid.bid_status = BidStatus::Rejected;
                // Store rejected bid in storage
                let mut prover_profile = self.storage.get_prover_profile(&bid.prover_name)?;
                prover_profile.bids.push(bid.clone());
                self.storage.update_prover_profile(&prover_profile)?;
                info!("❌ Bid rejected, contest is not live");
            }
        }
        info!("🏁 Bid processing task completed");
        Ok(())
    }

    pub async fn start_new_contest(&self) -> Result<(), anyhow::Error> {
        info!("🎯 start_new_contest task started!");
        
        loop {
            info!("🔄 start_new_contest loop iteration");
            
            // Get contest status with minimal lock time
            let contest_status = {
                let contest = self.execution_interface.lock().await.current_contest.clone();
                info!("Contest {} status: {:?}", contest.contest_id, contest.status);
                contest.status
            };
            
            match contest_status {
                ContestStatus::Live => {
                    info!("🏁 contest is live, waiting for CONTEST_DURATION");
                    // wait for the contest duration to start a new contest
                    sleep(Duration::from_millis(CONTEST_DURATION)).await;
                    
                    // End contest with minimal lock time
                    {
                        let mut execution = self.execution_interface.lock().await;
                        info!("🏁 Ending contest with {} bids", execution.current_contest.bids.len());
                        execution.end_contest();
                    }
                    info!("✅ Contest ended");
                }
                ContestStatus::Ended => {
                    info!("📦 storing contest and waiting for PROOF_DURATION");
                    // Store contest with minimal lock time
                    let contest_to_store = {
                        let contest = self.execution_interface.lock().await.current_contest.clone();
                        contest
                    };
                    self.storage.store_contest(&contest_to_store)?;
                    
                    // wait for the proof duration to start a new contest
                    sleep(Duration::from_millis(PROOF_DURATION)).await;
                    
                    let next_id = {
                        let contest = self.execution_interface.lock().await.current_contest.clone();
                        contest.contest_id + 1
                    };
                    
                    // Start new contest with minimal lock time
                    {
                        let mut execution = self.execution_interface.lock().await;
                        execution.start_contest(next_id);
                    }
                    
                    info!("🚀 New contest started with ID: {}", next_id);
                }
                ContestStatus::NotStarted => {
                    info!("🎬 Starting first contest");
                    let next_id = 0; // Start from contest ID 0
                    
                    // Start contest with minimal lock time
                    {
                        let mut execution = self.execution_interface.lock().await;
                        execution.start_contest(next_id);
                    }
                    info!("🎬 First contest started with ID: {}", next_id);
                }
            }
        }
    }

    pub async fn process_contest_completion(&self, contest_sender: Sender<Contest>) -> Result<(), anyhow::Error> {
        info!("🎯 process_contest_completion task started!");

        let (internal_contest_sender, mut internal_contest_receiver) = mpsc::channel::<Contest>(100);
        let execution_interface = self.execution_interface.clone();
        tokio::spawn(async move {
            loop {
                let (contest_ended, contest) = {
                    let contest = execution_interface.lock().await.current_contest.clone();
                    (contest.status == ContestStatus::Ended, contest)
                };
                if contest_ended {
                    info!("🏆 Contest ended, sending to channel");
                    if let Err(e) = internal_contest_sender.send(contest).await {
                        error!("Failed to send contest: {}", e);
                        break;
                    }
                    // Wait for the next contest to start before checking again
                    loop {
                        let contest = execution_interface.lock().await.current_contest.clone();
                        if contest.status == ContestStatus::Live {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        });

        // The following logic is OUTSIDE the loop
        // Get winner, store contest, send via channel, etc.
        while let Some(mut contest) = internal_contest_receiver.recv().await {
            let winner = {
                info!("🔍 Getting winner, bids count: {}", &contest.bids.len());
                contest.get_winner()
            };

            if let Some(winner) = winner {
                
                self.store_contest_in_redis(&contest).await;
                info!("🏆 Winner found: {:?}", winner.prover_name);

                if let Err(e) = contest_sender.send(contest).await {
                    error!("Failed to send contest: {}", e);
                } else {
                    info!("📤 Contest with winner sent via channel to RPC");
                }
            } else {
                info!("❌ No winner found - no bids");
            }
        }

        Ok(())
    }

    pub async fn store_contest_in_redis(&self, contest: &Contest) {
        // Store the completed contest in Redis
        if let Err(e) = self.storage.store_contest(contest) {
            log::error!("Failed to store contest in Redis: {}", e);
        }
    }

    pub async fn process_proof(&self, mut proof_receiver: Receiver<ProofData>, mut proof_status_sender: Sender<ProofData>) -> Result<(), anyhow::Error> {
        info!("🎯 process_proof task started!");
        
        while let Some(mut proof_data) = proof_receiver.recv().await {
            info!(
                "Received proof: {:?} from {:?}",
                proof_data.proof, proof_data.proof_header.prover_name
            );
            
            // check if its within the proving window
            let current_contest = self.execution_interface.lock().await.current_contest.clone();
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

                // Send proof status via channel (no mutex needed)
                if let Err(e) = proof_status_sender.send(proof_data.clone()).await {
                    error!("Failed to send proof status: {}", e);
                }
                info!("Proof rejected: {:?}", proof_data.proof_header.prover_name);
            }

            match self.execution_interface.lock().await.verify_proof(proof_data.clone()) {
                Ok(_) => {
                    proof_data.proof_header.proof_status = ProofStatus::Accepted;
                    // add credit to the prover
                    let mut prover_profile = self.storage.get_prover_profile(&proof_data.proof_header.prover_name)?;
                    prover_profile.prover_credits += current_contest.reward;
                    self.storage.update_prover_profile(&prover_profile)?;

                    // Send proof status via channel (no mutex needed)
                    if let Err(e) = proof_status_sender.send(proof_data.clone()).await {
                        error!("Failed to send proof status: {}", e);
                    }
                    info!("Proof accepted: {:?}", proof_data.proof_header.prover_name);
                }
                Err(e) => {
                    info!("Proof verification failed: {}", e);
                }
            }
        }
        Ok(())
    }

    pub async fn start_rpc_server(&self, rpc_port: u16) -> Result<(), anyhow::Error> {
        let server_config = ServerConfigBuilder::new()
            .max_request_body_size(1024 * 1024 * 25) // 25MB
            .max_response_body_size(1024 * 1024 * 25) // 25MB
            .build();

        let server_builder = ServerBuilder::with_config(server_config);

        let url = format!("127.0.0.1:{}", rpc_port);

        let rpc_handler = self.rpc_interface.clone();

        let server = server_builder.build(url).await?;
        let address = server.local_addr().expect("failed to get address");
        let handle = server.start(rpc_handler.into_rpc());

        tokio::spawn(handle.stopped());
        info!("Succinct Verifier WebSocket RPC server started on {}", address);
        Ok(())
    }

    pub async fn start(storage_type: StorageType, rpc_port: u16) -> Result<(), anyhow::Error> {
        info!("Succinct Verifier is running");

        // Create channels for task communication first
        let (bid_sender, bid_receiver) = mpsc::channel(100);
        let (proof_sender, proof_receiver) = mpsc::channel(100);
        let (proof_status_sender, proof_status_receiver) = mpsc::channel(100);
        let (contest_sender, contest_receiver) = mpsc::channel(100);

        // Create storage
        let storage: Arc<dyn Storage> = match storage_type {
            StorageType::Redis => Arc::new(RedisStorage::new()?),
            StorageType::InMemory => Arc::new(InMemoryStorage::new()),
        };

        let execution_interface = Arc::new(Mutex::new(VerifierExecutorImpl::new()));

        // Create RPC interface with the actual channels
        let rpc_interface = ProverNetwork::new(
            Arc::clone(&storage),
            Contest::default(),
            proof_sender.clone(),
            bid_sender.clone(),
            contest_receiver,
            proof_status_receiver,
        );

        // Create orchestrator with the proper RPC interface
        let orchestrator = MainOrchestrator {
            storage,
            rpc_interface: rpc_interface.clone(),
            execution_interface,
        };

        // Start the RPC server
        orchestrator.start_rpc_server(rpc_port).await?;

        info!("🚀 Starting background tasks with channels...");

        // Spawn bid processing task
        let orchestrator_clone = orchestrator.clone();
        tokio::spawn(async move {
            info!("🎯 Spawning listen_and_process_bids task");
            let res = orchestrator_clone.listen_and_process_bids(bid_receiver).await;
            if let Err(e) = res {
                error!("❌ Error in listen_and_process_bids: {}", e);
            }
        });

        // Spawn contest management task (self-contained, no channel needed)
        let orchestrator_clone = orchestrator.clone();
        tokio::spawn(async move {
            info!("🎯 Spawning start_new_contest task");
            let res = orchestrator_clone.start_new_contest().await;
            if let Err(e) = res {
                error!("❌ Error in start_new_contest: {}", e);
            }
        });

        // Spawn proof processing task
        let orchestrator_clone = orchestrator.clone();
        tokio::spawn(async move {
            info!("🎯 Spawning process_proof task");
            let res = orchestrator_clone.process_proof(proof_receiver, proof_status_sender).await;
            if let Err(e) = res {
                error!("❌ Error in process_proof: {}", e);
            }
        });

        // Spawn contest completion task
        let orchestrator_clone = orchestrator.clone();
        tokio::spawn(async move {
            info!("🎯 Spawning process_contest_completion task");
            let res = orchestrator_clone.process_contest_completion(contest_sender).await;
            if let Err(e) = res {
                error!("❌ Error in process_contest_completion: {}", e);
            }
        });

        info!("✅ All background tasks spawned successfully with channels");
        
        // Keep the main task alive
        loop {
            sleep(Duration::from_secs(1)).await;
        }
    }
}
