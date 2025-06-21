use alloy_signer::Signer;
use jsonrpsee::client_transport::ws::{Url, WsTransportClientBuilder};
use jsonrpsee::core::client::SubscriptionClientT;
use jsonrpsee::core::client::{Client, ClientBuilder, ClientT};
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

use serde::{Deserialize, Serialize};

use jsonrpsee::rpc_params;
use log::{error, info};
use primitives::data_structure::{
    BidRequest, BidResponse, BidStatus, Contest, ProofData, ProofHeader, ProofStatus,
    ProverProfile, Team,
};
use redis::Client as RedisClient;
use redis::Commands;
use redis::ConnectionLike;
use sp1_sdk::{ProverClient as Sp1ProverClient, SP1Stdin};
use std::env;
use std::str::FromStr;
use std::time::Duration;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use alloy_primitives::{keccak256, Address};
use alloy_signer::k256::ecdsa::SigningKey;
use alloy_signer_local::{LocalSigner, PrivateKeySigner};

// ================================ STORAGE TRAIT ================================

#[derive(Debug, Clone)]
pub enum StorageType {
    Redis,
    InMemory,
}

pub trait Storage: Send + Sync {
    fn store_prover_account(&self, account: &ProverAccount) -> Result<(), anyhow::Error>;
    fn get_prover_account(&self) -> Result<ProverAccount, anyhow::Error>;
    fn is_connected(&self) -> bool;
}

// ================================ IN-MEMORY STORAGE ================================

struct InMemoryStorageInner {
    prover_account: HashMap<String, String>,
}

pub struct InMemoryStorage {
    inner: Arc<Mutex<InMemoryStorageInner>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(InMemoryStorageInner {
                prover_account: HashMap::new(),
            })),
        }
    }
}

impl Storage for InMemoryStorage {
    fn store_prover_account(&self, account: &ProverAccount) -> Result<(), anyhow::Error> {
        let serialized_account = serde_json::to_string(account)?;
        let mut inner = self.inner.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock storage: {}", e))?;
        inner.prover_account.insert("my_prover_account".to_string(), serialized_account);
        Ok(())
    }

    fn get_prover_account(&self) -> Result<ProverAccount, anyhow::Error> {
        let inner = self.inner.lock()
            .map_err(|e| anyhow::anyhow!("Failed to lock storage: {}", e))?;
        
        let account_json = inner.prover_account
            .get("my_prover_account")
            .ok_or_else(|| anyhow::anyhow!("No prover account found"))?;
        
        let account = serde_json::from_str::<ProverAccount>(account_json)?;
        Ok(account)
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
}

impl Storage for RedisStorage {
    fn store_prover_account(&self, account: &ProverAccount) -> Result<(), anyhow::Error> {
        let serialized_account = serde_json::to_string(account)?;
        let _ = self.client.clone().set::<String, String, String>(
            "my_prover_account".to_string(),
            serialized_account,
        )?;
        Ok(())
    }

    fn get_prover_account(&self) -> Result<ProverAccount, anyhow::Error> {
        if !self.client.is_open() {
            self.client
                .get_connection()
                .map_err(|e| anyhow::anyhow!("Failed to reconnect to redis: {}", e))?;
        }

        let account_json = self.client
            .clone()
            .get::<String, String>("my_prover_account".to_string())?;
        
        let account = serde_json::from_str::<ProverAccount>(&account_json)?;
        Ok(account)
    }

    fn is_connected(&self) -> bool {
        self.client.is_open()
    }
}

// ================================ CLIENT ================================

#[derive(Serialize, Deserialize)]
pub struct ProverAccount {
    pub prover_name: String,
    pub prover_address: String,
    pub signer: Vec<u8>,
}

pub struct ProverClient {
    ws_client: WsClient,
    client: Client,
    execution: ExecutionClient,
    storage_type: StorageType,
}

impl ProverClient {
    fn get_url() -> String {
        let url = std::env::var("VERIFIER_NODE_URL");
        url.unwrap()
    }

    async fn reconnect_ws_client(&mut self) -> Result<(), anyhow::Error> {
        let url = Self::get_url();
        self.ws_client = WsClientBuilder::default().build(url).await?;
        Ok(())
    }

    pub async fn new(storage_type: StorageType) -> Result<Self, anyhow::Error> {
        let url = Self::get_url();

        let ws_client = WsClientBuilder::default()
            .request_timeout(Duration::from_secs(120))
            .connection_timeout(Duration::from_secs(500))
            .build(url.clone())
            .await?;

        let uri = Url::parse(&url)?;
        let (tx, rx) = WsTransportClientBuilder::default().build(uri).await?;
        let client = ClientBuilder::default()
            .request_timeout(Duration::from_secs(120))
            .build_with_tokio(tx, rx);

        let execution = ExecutionClient::new(storage_type.clone())?;
        Ok(Self {
            ws_client,
            client,
            execution,
            storage_type,
        })
    }

    pub async fn register_prover(
        &self,
        prover_name: String,
        prover_team: Option<Team>,
    ) -> Result<(), anyhow::Error> {
        // check if prover is already registered
        if !self.execution.storage.is_connected() {
            return Err(anyhow::anyhow!("Storage not connected"));
        }
        
        // Try to get existing account, if it exists, return early
        if let Ok(_) = self.execution.storage.get_prover_account() {
            return Ok(());
        }
        
        // register locally first
        let key = PrivateKeySigner::random();
        let credential = key.clone().into_credential().to_bytes().to_vec();
        let prover_address = key.address().to_string();

        let prover_account = ProverAccount {
            prover_name: prover_name.clone(),
            prover_address: prover_address.clone(),
            signer: credential,
        };

        self.execution.register_prover(prover_account)?;
        let prover_team = prover_team.unwrap_or(Team::Blue);

        let params = rpc_params!(prover_name, prover_team, prover_address);
        self.client
            .request("register_prover", params)
            .await?;
        Ok(())
    }

    pub async fn submit_bid_and_proof(&mut self, bid_amount: u64) -> Result<(), anyhow::Error> {
        if !self.ws_client.is_connected() {
            self.reconnect_ws_client().await?;
        }

        let prover_account = self.execution.get_prover_account()?;

        let bid = BidRequest {
            bid_amount,
            prover_address: prover_account.prover_address,
            prover_name: prover_account.prover_name,
            bid_status: BidStatus::Pending,
        };

        let params = rpc_params!(bid);
        let mut subscription = self
            .ws_client
            .subscribe::<BidResponse, _>("submit_bid", params, "unsub_submit_bid")
            .await?;
        loop {
            let bid_response = subscription.next().await;
            if let Some(Ok(ref bid_response)) = bid_response {
                let proof = self.execution.generate_proof(bid_response.clone()).await?;
                let params = rpc_params!(proof);
                self.client.request::<(), _>("submit_proof", params).await?;
            }
            if let Some(Err(e)) = bid_response {
                error!("Error watching bid response: {:?}", e);
            }
        }
    }

    // ================================ GETTERS ================================

    pub async fn watch_proof_status(&mut self) -> Result<(), anyhow::Error> {
        if !self.ws_client.is_connected() {
            self.reconnect_ws_client().await?;
        }
        let mut subscription = self
            .ws_client
            .subscribe_to_method::<ProofStatus>("watch_proof_status")
            .await?;

        loop {
            let proof_status = subscription.next().await;
            if let Some(Ok(ref proof_status)) = proof_status {
                info!("Current Proof Generation Status: {:?}", proof_status);
            }
            if let Some(Err(e)) = proof_status {
                error!("Error watching proof status: {:?}", e);
            }
        }
    }

    pub async fn watch_current_contest(&mut self) -> Result<(), anyhow::Error> {
        if !self.ws_client.is_connected() {
            self.reconnect_ws_client().await?;
        }
        let mut subscription = self
            .ws_client
            .subscribe_to_method::<Contest>("watch_current_contest")
            .await?;
        loop {
            let contest = subscription.next().await;
            if let Some(Ok(ref contest)) = contest {
                info!("Current Contest: {:?}", contest);
            }
            if let Some(Err(e)) = contest {
                error!("Error watching current contest: {:?}", e);
            }
        }
    }

    pub async fn get_provers(&self) -> Result<Vec<ProverProfile>, anyhow::Error> {
        let params = rpc_params!();
        let provers = self
            .client
            .request::<Vec<ProverProfile>, _>("get_provers", params)
            .await?;
        Ok(provers)
    }
}

// =============================== EXECUTION ================================
pub struct ExecutionClient {
    pub storage: Box<dyn Storage>,
}

impl ExecutionClient {
    pub fn new(storage_type: StorageType) -> Result<Self, anyhow::Error> {
        let storage: Box<dyn Storage> = match storage_type {
            StorageType::Redis => Box::new(RedisStorage::new()?),
            StorageType::InMemory => Box::new(InMemoryStorage::new()),
        };
        Ok(Self { storage })
    }

    pub fn register_prover(&self, prover_account: ProverAccount) -> Result<(), anyhow::Error> {
        self.storage.store_prover_account(&prover_account)?;
        Ok(())
    }

    pub fn get_prover_account(&self) -> Result<ProverAccount, anyhow::Error> {
        self.storage.get_prover_account()
    }

    pub async fn generate_proof(&self, req: BidResponse) -> Result<ProofData, anyhow::Error> {
        let prover_client = Sp1ProverClient::from_env();

        let start_time = std::time::Instant::now();
        let (_public_values, report) = prover_client
            .execute(
                req.program.as_slice(),
                &SP1Stdin::from(req.input.as_slice()),
            )
            .run()?;
        let end_time = std::time::Instant::now();
        let duration = end_time.duration_since(start_time);
        info!(
            "executed program with {} cycles in {:?} seconds",
            report.total_instruction_count(),
            duration.as_secs_f64()
        );

        let start_time = std::time::Instant::now();
        let (proving_key, verifying_key) = prover_client.setup(req.program.as_slice());
        let proof = prover_client
            .prove(&proving_key, &SP1Stdin::from(req.input.as_slice()))
            .run()?;
        let end_time = std::time::Instant::now();

        info!(
            "executed program with {} cycles in {:?} seconds",
            report.total_instruction_count(),
            start_time.duration_since(end_time).as_secs_f64()
        );

        let start_time = std::time::Instant::now();
        let (proving_key, verifying_key) = prover_client.setup(req.program.as_slice());
        let proof = prover_client
            .prove(&proving_key, &SP1Stdin::from(req.input.as_slice()))
            .run()?;
        let end_time = std::time::Instant::now();
        let duration = end_time.duration_since(start_time);
        info!(
            "generated proof: {:?} in {:?} seconds",
            proof.bytes(),
            duration.as_secs_f64()
        );

        // load prover keys
        let prover_profile = self.get_prover_account()?;
        let signer_key = SigningKey::from_slice(&prover_profile.signer)?;

        let signer = LocalSigner::new_with_credential(
            signer_key,
            Address::from_str(&prover_profile.prover_address)?,
            None,
        );
        let proof_signature = signer.sign_hash(&keccak256(proof.clone().bytes())).await?;

        let proof_data = ProofData {
            proof_header: ProofHeader {
                proof_timestamp: std::time::Instant::now().elapsed().as_secs(),
                proof_signature: proof_signature.to_string().as_bytes().to_vec(),
                prover_address: prover_profile.prover_address,
                prover_name: prover_profile.prover_name,
                proof_status: ProofStatus::Pending,
            },
            proof: proof.into(),
            verify_key: verifying_key,
        };
        Ok(proof_data)
    }
}
