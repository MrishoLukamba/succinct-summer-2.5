# Succinct SP1 Prover Network

An offchain succinct SP1 prover network that enables proving Ethereum blocks and producing STARK proofs for verification. This system allows provers to participate in contests to generate zero-knowledge proofs of Ethereum block execution, which can then be verified on-chain.

Currently it supports only 1 precompiled Eth runtime with some transaction input

## Architecture

The project consists of two main components: **Prover** and **Verifier**, each running as separate services that communicate via JSON-RPC.

### Prover

The prover is a client application that connects to the verifier network to participate in proving contests.

**Key Features:**
- **JSON-RPC Client**: Uses `jsonrpsee` to communicate with the verifier
- **SP1 Integration**: Leverages the SP1 SDK for proof generation
- **Interactive CLI**: Command-line interface for prover operations
- **Local Storage**: Redis-based local storage for prover profiles and accounts

**API Endpoints:**
- `register_prover` - Register a new prover with name, team, and address
- `submit_bid` - Submit a bid to participate in a contest
- `submit_proof` - Submit generated proof data
- `watch_current_contest` - Subscribe to contest updates
- `watch_proof_status` - Subscribe to proof status updates
- `get_provers` - Get list of all registered provers

**CLI Commands:**
```bash
# Register a prover
register --name <NAME> [--team <TEAM>]

# Submit a bid
bid --amount <AMOUNT>

# Watch proof status
watch-proof

# Watch current contest
watch-contest

# Get all provers
get-provers

# Show help
help

# Exit
quit
```

### Verifier

The verifier is a server application that orchestrates proving contests and manages the network.

**Key Features:**
- **JSON-RPC Server**: Provides RPC endpoints for prover interactions
- **Contest Management**: Manages contest lifecycle and winner selection
- **Proof Verification**: Verifies submitted proofs using SP1
- **Storage Layer**: Redis-based storage for provers, contests, and proof data

**Contest Lifecycle:**
1. **Not Started**: Initial state, waiting to begin
2. **Live**: Contest is active, accepting bids from provers
3. **Ended**: Contest finished, winner selected, proof generation period begins
4. **Proof Window**: Winners generate proofs within the allocated time

**Interaction Flow:**
1. Provers register with the network
2. Verifier starts a new contest
3. Provers submit bids during the live phase
4. Contest ends and winner is selected
5. Winner receives program and input data
6. Winner generates proof and submits it
7. Verifier validates the proof and awards credits

## Prerequisites

### System Requirements
- **RAM**: At least 6GB RAM
- **OS**: Linux, macOS, or Windows with WSL
- **Network**: Internet connection for dependencies

### Software Requirements

#### Rust
Install Rust using rustup:

```bash
# Install rustup (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Or on Windows
# Download and run rustup-init.exe from https://rustup.rs

# Reload shell environment
source ~/.bashrc  # or source ~/.zshrc

# Verify installation
rustc --version
cargo --version
```

#### Redis Database
Install Redis:

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install redis-server
sudo systemctl start redis-server
sudo systemctl enable redis-server
```

**macOS:**
```bash
# Using Homebrew
brew install redis
brew services start redis

# Or manually
redis-server
```

**Windows:**
```bash
# Using WSL or Docker
docker run -d -p 6379:6379 redis:alpine
```

**Verify Redis:**
```bash
redis-cli ping
# Should return: PONG
```

## Setup Instructions

### 1. Clone the Repository
```bash
git clone <repository-url>
cd succint-summer-2.5
```

### 2. Environment Configuration
Create a `.env` file in the project root:

```bash
# Redis configuration
REDIS_URL=redis://localhost:6379

# Verifier node URL for prover to connect to
VERIFIER_NODE_URL=ws://localhost:5789

# SP1 Prover configuration
SP1_PROVER_URL=https://prover.succinct.xyz
```

### 3. Build the Project
```bash
# Build all components
cargo build --release

# Or build individual components
cargo build --release -p verifier
cargo build --release -p prover
```

### 4. Start the Verifier
```bash
# Start the verifier server on port 5789
./target/release/verifier --port 5789
```

You should see:
```
Succinct Verifier WebSocket RPC server started on 127.0.0.1:5789
```

### 5. Start the Prover
```bash
# Start the prover client
./target/release/prover --prover-name <YOUR_NAME>
```

You should see the ASCII art banner and CLI prompt:
```
> 
```

## Usage Instructions

### Verifier Commands

The verifier runs as a server and doesn't have interactive commands. It automatically:
- Starts new contests
- Manages contest lifecycle
- Processes bids and proofs
- Maintains prover profiles

### Prover Commands

Once the prover CLI is running, you can use these commands:

#### Register a Prover
```bash
> register --name Alice --team Blue
✅ Prover registered successfully!
```

#### Submit a Bid
```bash
> bid --amount 1000
✅ Bid submitted successfully!
```

#### Monitor Contest Status
```bash
> watch-contest
✅ Contest monitoring started in background.
```

#### Monitor Proof Status
```bash
> watch-proof
✅ Proof monitoring started in background.
```

#### Get All Provers
```bash
> get-provers
📋 Registered Provers:
  name: Alice credits: 1000 no_bids: 2
  name: Bob credits: 500 no_bids: 1
```

#### Background Task Management
```bash
# Stop all background monitoring
> stop-watching

# Check background task status
> status

# Show help
> help
```

## Network Interaction Flow

1. **Start Verifier**: The verifier begins accepting connections and starts contest cycles
2. **Start Prover**: Connect to the verifier network
3. **Register**: Register your prover identity
4. **Monitor**: Watch for active contests
5. **Bid**: Submit bids when contests are live
6. **Generate**: If you win, generate proofs using SP1
7. **Submit**: Submit proofs for verification
8. **Earn**: Receive credits for successful proofs

## Troubleshooting

### Common Issues

**Connection Refused:**
- Ensure Redis is running: `redis-cli ping`
- Check verifier is started before prover
- Verify port 5789 is available

**Registration Errors:**
- Check Redis connection
- Ensure prover name is unique
- Verify environment variables are set

**Build Errors:**
- Update Rust: `rustup update`
- Clean and rebuild: `cargo clean && cargo build --release`

### Logs
- Verifier logs: `succinct-log.log`
- Prover logs: `succinct-log.log`
- Redis logs: Check system logs or Redis CLI

## Development

### Project Structure
```
succint-summer-2.5/
├── Cargo.toml              # Workspace configuration
├── primitives/             # Shared data structures
├── prover/                 # Prover client application
│   ├── src/
│   │   ├── main.rs         # CLI interface
│   │   └── client.rs       # RPC client
└── verifier/               # Verifier server application
    ├── src/
    │   ├── main.rs         # Server orchestration
    │   ├── networking.rs   # RPC server
    │   └── execution.rs    # Proof verification
```

### Adding Features
- **New RPC Methods**: Add to `ProverNetworkRpc` trait in `networking.rs`
- **Storage Operations**: Extend `StorageLayer` in `main.rs`
- **CLI Commands**: Add to `Commands` enum in prover's `main.rs`

## License

[MIT licence] 