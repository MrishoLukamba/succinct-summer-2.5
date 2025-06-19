use alloy_primitives::{keccak256, Address, Signature as EcdsaSignature, B256};
use serde::{Deserialize, Serialize};

pub const ETH_SIG_MSG_PREFIX: &str = "\x19Ethereum Signed Message:\n";
pub const CONTEST_DURATION: u64 = 1000 * 40; // 40 seconds
pub const PROOF_DURATION: u64 = 1000 * 20; // 20 seconds, this serves as also the time between 1 contest to another
pub const CREDIT_SLASH: u64 = 1000; // 1000 credits per invalid proof

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ProofData {
    pub proof_header: ProofHeader,
    pub proof: Vec<u8>,
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
        let mut msg = Vec::<u8>::new();
        msg.extend_from_slice(ETH_SIG_MSG_PREFIX.as_bytes());
        msg.extend_from_slice(self.proof.len().to_string().as_bytes());
        msg.extend_from_slice(self.proof.as_slice());

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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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
    pub fn calculate_winner(&self) -> Option<BidRequest> {
        todo!()
    }
    pub fn is_live(&self) -> bool {
        self.start_time <= self.end_time && self.end_time - self.start_time < CONTEST_DURATION
    }
    pub fn start_contest(&mut self) {
        self.start_time = std::time::Instant::now().elapsed().as_secs();
        self.end_time = self.start_time + CONTEST_DURATION;
    }
    pub fn end_contest(&mut self) {
        self.end_time = std::time::Instant::now().elapsed().as_secs();
    }
    pub fn add_bid(&mut self, bid: BidRequest) {
        self.bids.push(bid);
    }
    pub fn get_bids(&self) -> &Vec<BidRequest> {
        &self.bids
    }
    pub fn get_winner(&self) -> Option<&BidRequest> {
        self.winner.as_ref()
    }
}
