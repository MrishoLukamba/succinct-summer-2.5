use anyhow::Error;
use primitives::data_structure::{BidRequest, Contest, ProofData, CONTEST_DURATION, CONTEST_REWARD, ContestStatus};
use rand::Rng;
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

    fn add_bid(&mut self, bid: BidRequest) -> bool{
        let found = self.current_contest.bids.iter().find(|b| b.prover_address == bid.prover_address);
        if found.is_some() {
            return false;
        }
        self.current_contest.bids.push(bid);
        true
    }

    fn get_winner(&mut self) -> Option<BidRequest> {
        let mut bidders: Vec<(String,u64)> = Vec::new();
        let random_number:u32 = rand::thread_rng().gen_range(0..self.current_contest.bids.len()).try_into().unwrap();
        let bids = self.current_contest.bids.clone();

        let total_bid_amount = self.current_contest.bids.iter().map(|b| b.bid_amount.pow(random_number)).sum::<u64>();
        bids.iter().for_each(|b|{
            let percentage_bid = b.bid_amount.pow(random_number) / total_bid_amount;
            bidders.push((b.prover_name.clone(), percentage_bid));
        });
        bidders.sort_by_key(|(_,amount)| *amount); 
        let range = bidders.last().unwrap().1 - bidders.first().unwrap().1;
        let index_winner: usize = (range % (random_number as u64)) as usize;
        let winner = &bidders[index_winner];
        let winner_bid = bids.iter().find(|b| b.prover_name == winner.0).unwrap();
        self.current_contest.winner = Some(winner_bid.clone());
        Some(winner_bid.clone())
    }

    fn verify_proof(&self, proof: ProofData) -> Result<(), Error> {
        let prover_client = ProverClient::from_env();
        match prover_client.verify(&proof.proof, &proof.verify_key) {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow::anyhow!("Error verifying proof: {:?}", e)),
        }
        
    }
}
