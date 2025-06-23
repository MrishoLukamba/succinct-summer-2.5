use alloy_primitives::{keccak256, Address, Signature as EcdsaSignature, B256};
use serde::{Deserialize, Serialize};
use sp1_sdk::{SP1ProofWithPublicValues, SP1VerifyingKey};
use rand::Rng;
use log::info;

pub const ETH_SIG_MSG_PREFIX: &str = "\x19Ethereum Signed Message:\n";
pub const CONTEST_DURATION: u64 = 1000 * 10; // 10 seconds
pub const PROOF_DURATION: u64 = 1000 * 10; // 10 seconds, this serves as also the time between 1 contest to another
pub const CREDIT_SLASH: u64 = 1000; // 1000 credits per invalid proof
pub const CONTEST_REWARD: u64 = 1500; // 1500 credits per contest

pub const ETH_BLOCK_PROGRAM: &[u8] = include_bytes!("../../artifacts/rsp");
pub const ETH_TXN_INPUT: &[u8] = include_bytes!("../../artifacts/buffer.bin");
#[derive(Serialize, Deserialize, Clone)]
pub struct ProofData {
    pub proof_header: ProofHeader,
    pub proof: SP1ProofWithPublicValues,
    pub verify_key: SP1VerifyingKey,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ProofHeader {
    pub proof_timestamp: u64,
    pub proof_signature: Vec<u8>,
    pub prover_address: String,
    pub prover_name: String,
    pub proof_status: ProofStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ProofStatus {
    Accepted,
    Rejected,
    Pending,
}

impl ProofData {
    pub fn sanity_check(&self) -> Result<(), anyhow::Error> {
        // verify proof with proof signature and prover address
        let binding = serde_json::to_string(&self.proof).unwrap();
        let encoded_proof = binding.as_bytes();

        let mut msg = Vec::<u8>::new();
        msg.extend_from_slice(ETH_SIG_MSG_PREFIX.as_bytes());
        msg.extend_from_slice(encoded_proof.len().to_string().as_bytes());
        msg.extend_from_slice(encoded_proof);

        let hashed = keccak256(&msg);
        let sig = EcdsaSignature::try_from(self.proof_header.proof_signature.as_slice())
            .map_err(|e| anyhow::anyhow!("Invalid signature: {}", e))?;

        match sig.recover_address_from_prehash(<&B256>::from(&hashed)) {
            Ok(pub_key) => {
                let prover_address =
                    Address::from_slice(&self.proof_header.prover_address.as_bytes());
                if pub_key != prover_address {
                    return Err(anyhow::anyhow!("Invalid signature"));
                }
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Invalid signature: {}", e));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct BidRequest {
    pub bid_amount: u64,
    pub prover_address: String,
    pub prover_name: String,
    pub bid_status: BidStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct BidResponse {
    pub program: Vec<u8>,
    pub input: Vec<u8>,
    pub contest_id: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum BidStatus {
    Pending,
    Accepted,
    Rejected,
}

impl BidStatus {
    pub fn reject() -> Self {
        Self::Rejected
    }

    pub fn accept() -> Self {
        Self::Accepted
    }

    pub fn pending() -> Self {
        Self::Pending
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProverProfile {
    pub prover_address: String,
    pub prover_name: String,
    pub prover_credits: u64,
    pub prover_team: Team,
    pub bids: Vec<BidRequest>,
    pub proofs: Vec<ProofData>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Team {
    Blue,
    Pink,
    Green,
    Orange,
    Purple,
}

impl From<String> for Team {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Blue" => Self::Blue,
            "Pink" => Self::Pink,
            "Green" => Self::Green,
            "Orange" => Self::Orange,
            "Purple" => Self::Purple,
            _ => Self::Blue,
        }
    }
}

impl ProverProfile {
    pub fn new(prover_name: String, prover_team: Team, prover_address: String) -> Self {
        Self {
            prover_address: prover_address.to_string(),
            prover_name,
            prover_credits: 0,
            prover_team,
            bids: Vec::new(),
            proofs: Vec::new(),
        }
    }
}

impl Default for Team {
    fn default() -> Self {
        Self::Blue
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Contest {
    pub contest_id: u64,
    pub start_time: u64,
    pub end_time: u64,
    pub bids: Vec<BidRequest>,
    pub winner: Option<BidRequest>,
    pub reward: u64,
    pub status: ContestStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ContestStatus {
    Live,
    Ended,
    NotStarted,
}
impl Default for ContestStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

impl Contest {
    pub fn is_live(&self) -> bool {
        self.status == ContestStatus::Live
    }
    pub fn start_contest(&mut self) {
        self.start_time = std::time::Instant::now().elapsed().as_secs();
        self.end_time = self.start_time + CONTEST_DURATION;
    }
    pub fn end_contest(&mut self) {
        self.status = ContestStatus::Ended;
    }
    pub fn add_bid(&mut self, bid: BidRequest) {
        self.bids.push(bid);
    }
    pub fn get_bids(&self) -> &Vec<BidRequest> {
        &self.bids
    }
    pub fn get_winner(&mut self) -> Option<BidRequest> {
        if self.bids.is_empty() {
            info!("No bids to select winner from");
            return None;
        }

        let mut bidders: Vec<(String, u64)> = Vec::new();
        let random_number: u32 = rand::thread_rng()
            .gen_range(0..self.bids.len())
            .try_into()
            .unwrap();
        
        let bids = self.bids.clone();

        let total_bid_amount = self
            .bids
            .iter()
            .map(|b| b.bid_amount.pow(random_number))
            .sum::<u64>();

        if total_bid_amount == 0 {
            info!("Total bid amount is 0, selecting random winner");
            let random_index = rand::thread_rng().gen_range(0..bids.len());
            let winner_bid = &bids[random_index];
            self.winner = Some(winner_bid.clone());
            return Some(winner_bid.clone());
        }

        bids.iter().for_each(|b| {
            let percentage_bid = b.bid_amount.pow(random_number) / total_bid_amount;
            bidders.push((b.prover_name.clone(), percentage_bid));
        });

        if bidders.is_empty() {
            info!("No bidders after calculation");
            return None;
        }

        bidders.sort_by_key(|(_, amount)| *amount);
        
        if bidders.len() == 1 {
            // Only one bidder, they win
            let winner = &bidders[0];
            let winner_bid = bids.iter().find(|b| b.prover_name == winner.0).unwrap();
            self.winner = Some(winner_bid.clone());
            return Some(winner_bid.clone());
        }

        let range = bidders.last().unwrap().1 - bidders.first().unwrap().1;
        let index_winner: usize = (range % (random_number as u64)) as usize;
        
        // Ensure index_winner is within bounds
        let index_winner = index_winner % bidders.len();
        
        let winner = &bidders[index_winner];
        let winner_bid = bids.iter().find(|b| b.prover_name == winner.0).unwrap();
        self.winner = Some(winner_bid.clone());
        Some(winner_bid.clone())
    }
}
