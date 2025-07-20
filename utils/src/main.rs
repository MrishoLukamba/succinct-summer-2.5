#![no_main]
sp1_zkvm::entrypoint!(main);

use polkadot_primitives::{v8::{executor_params::{ExecutorParam, ExecutorParams}, PvfExecKind, PvfPrepKind}};
use polkadot_node_core_pvf_common::executor_interface::create_runtime_from_artifact_bytes;
use sc_executor_common::wasm_runtime::WasmModule;

pub fn main() {
    // handling inpiut
    let wasm_bytes = sp1_zkvm::io::read_vec();
    let block_bytes = sp1_zkvm::io::read_vec();

    let mut executor_params = Vec::<ExecutorParam>::new();
    executor_params.push(ExecutorParam::MaxMemoryPages(65536));
    executor_params.push(ExecutorParam::PrecheckingMaxMemory(5 * 1024 * 1024));
    executor_params.push(ExecutorParam::StackLogicalMax(10000));
    executor_params.push(ExecutorParam::StackNativeMax(1024 * 1024));
    executor_params.push(ExecutorParam::WasmExtBulkMemory);
    executor_params.push(ExecutorParam::PvfPrepTimeout(PvfPrepKind::Prepare, 500));
    executor_params.push(ExecutorParam::PvfExecTimeout(PvfExecKind::Approval, 2000));
    
    let executor_params = ExecutorParams::from(executor_params.as_slice());

    unsafe {

        let mut ext = sp_state_machine::BasicExternalities::new_empty();
        match sc_executor::with_externalities_safe(&mut ext, ||{
            let runtime = create_runtime_from_artifact_bytes(wasm_bytes.as_slice(), &executor_params)?;
            runtime.new_instance()?.call("Core_execute_block", &block_bytes)
        }){
            Ok(result) => {
                match result {
                    Ok(result) => {
                        sp1_zkvm::io::commit(&result);
                    }
                    Err(e) => {
                        
                    }
                }
            }
            Err(e) => {
                
            }
        }
    }

}
