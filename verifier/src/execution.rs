use primitives::data_structure::{BidRequest, Contest, ProofData};
use anyhow::Error;

pub trait VerifierExecutor {
    fn start_contest(&self);
    fn end_contest(&self) -> Result<Contest, Error>;
    fn add_bid(&self, bid: BidRequest) -> Result<(), Error>;
    fn get_winner(&self) -> Result<Contest, Error>;
    fn verify_proof(&self, proof: ProofData) -> Result<(), Error>;
}

pub struct VerifierExecutorImpl {
    current_contest: Option<Contest>,
}

// impl VerifierExecutor for VerifierExecutorImpl {
//     fn start_contest(&self) {
//         todo!()
//     }

//     fn end_contest(&self) -> Result<Contest, Error> {
//         todo!()
//     }

//     fn add_bid(&self, bid: BidRequest) -> Result<(), Error> {
//         todo!()
//     }

//     fn get_winner(&self) -> Result<Contest, Error> {
//         todo!()
//     }

//     fn verify_proof(&self, proof: ProofData) -> Result<(), Error> {
//         todo!()
//     }
// }