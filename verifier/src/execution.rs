use anyhow::Error;
use log::info;
use primitives::data_structure::{
    BidRequest, Contest, ContestStatus, ProofData, CONTEST_DURATION, CONTEST_REWARD,
};

use sp1_sdk::{ProverClient, SP1ProvingKey, SP1VerifyingKey};
pub trait VerifierExecutor {
    fn start_contest(&mut self, next_id: u64);
    fn end_contest(&mut self);
    fn add_bid(&mut self, bid: BidRequest) -> bool;
    fn get_winner(&mut self) -> Option<BidRequest>;
    fn verify_proof(&self, proof: ProofData) -> Result<(), Error>;
}

#[derive(Clone)]
pub struct VerifierExecutorImpl {
    pub current_contest: Contest,
}

impl VerifierExecutorImpl {
    pub fn new() -> Self {
        Self {
            current_contest: Contest::default(),
        }
    }
}

impl VerifierExecutor for VerifierExecutorImpl {
    fn start_contest(&mut self, next_id: u64) {
        info!("creating new contest");
        let new_contest = Contest {
            contest_id: next_id,
            start_time: std::time::Instant::now().elapsed().as_secs(),
            end_time: std::time::Instant::now().elapsed().as_secs() + CONTEST_DURATION,
            bids: vec![],
            winner: None,
            reward: CONTEST_REWARD,
            status: ContestStatus::Live,
        };
        self.current_contest = new_contest;
    }

    fn end_contest(&mut self) {
        self.current_contest.end_contest();
    }

    fn add_bid(&mut self, bid: BidRequest) -> bool {
        let found = self
            .current_contest
            .bids
            .iter()
            .find(|b| b.prover_address == bid.prover_address);
        if found.is_some() {
            info!("bid already exists");
            return false;
        }
        self.current_contest.bids.push(bid);
        true
    }

    fn get_winner(&mut self) -> Option<BidRequest> {
        self.current_contest.get_winner()
    }

    fn verify_proof(&self, proof: ProofData) -> Result<(), Error> {
        let prover_client = ProverClient::local();
        match prover_client.verify(&proof.proof, &proof.verify_key) {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Error verifying proof: {:?}", e)),
        }
    }
}
