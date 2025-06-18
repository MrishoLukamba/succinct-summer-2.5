#[tokio::main]
async fn main() {
    println!("Hello, world!");
}

// ================================ ORCHESTRATOR ================================

// STORAGE LAYOUT ( we are using redis )
// 1. provers: Map<String, ProverProfile>
// 2. contests: Vec<Contest>
// temp storage
// 1. current_contest: Contest

mod networking;
mod execution;

use redis::Client as RedisClient;
use crate::networking::ProverNetwork;
use crate::execution::VerifierExecutorImpl;
use primitives::data_structure::{BidRequest, BidResponse, Contest};
use tokio::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MainOrchestrator {
    pub redis_client: RedisClient,
    pub current_contest: Arc<Mutex<Contest>>,
    pub rpc_interface: ProverNetwork,
    pub execution_interface: VerifierExecutorImpl,
    pub bid_receiver_channel: Arc<Mutex<Receiver<BidRequest>>>,
    pub bid_response_sender_channel: Arc<Mutex<Sender<BidResponse>>>,
}








