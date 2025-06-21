mod client;
use clap::{Parser, Subcommand};
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

use log::LevelFilter;
use simplelog::*;
use std::fs::File;
use primitives::data_structure::Team;

use client::ProverClient;

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
    pub prover_name: String,
    #[arg(short, long)]
    pub prover_team: Option<String>,
}

#[derive(Parser)]
#[command(name = "")]
#[command(about = "Interactive Prover CLI")]
struct CliCommand {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a prover with optional team
    Register {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        team: Option<String>,
    },
    /// Submit a bid with specified amount
    Bid {
        #[arg(short, long)]
        amount: u64,
    },
    /// Watch proof status (starts monitoring)
    WatchProof,
    /// Watch current contest (starts monitoring)
    WatchContest,
    /// Get list of all provers
    GetProvers,
    /// Show help
    Help,
    /// Exit the program
    Quit,
}

async fn handle_command(
    command: Commands,
    client: Arc<Mutex<ProverClient>>,
) -> Result<bool, anyhow::Error> {
    match command {
        Commands::Register { name, team } => {
            println!("Registering prover: {} with team: {:?}", name, team);
            let cloned_client = client.clone();
            let client = cloned_client.lock().await;
            let team = if let Some(team) = team {
                let t: Team = team.into();
                Some(t)
            } else {
                None
            };
            match client.register_prover(name, team).await {
                Ok(_) => println!("✅ Prover registered successfully!"),
                Err(e) => println!("❌ Registration failed: {}", e),
            }
        }

        Commands::Bid { amount } => {
            println!("Submitting bid with amount: {}", amount);
            // Note: This function runs indefinitely, so you might want to handle it differently
            // Perhaps spawn it as a background task
            let cloned_client = client.clone();
            let mut client = cloned_client.lock().await;
            match client.submit_bid_and_proof(amount).await {
                Ok(_) => println!("✅ Bid submitted successfully!"),
                Err(e) => println!("❌ Bid submission failed: {}", e),
            }
        }

        Commands::WatchProof => {
            println!("Starting to watch proof status...");
            println!("Press Ctrl+C to stop watching.");
            // Clone the client and move it into the async task
            let cloned_client = client.clone();
            tokio::spawn(async move {
                let mut client = cloned_client.lock().await;
                if let Err(e) = client.watch_proof_status().await {
                    println!("❌ Error watching proof status: {}", e);
                }
            });
            println!("✅ Proof status monitoring started in background.");
        }

        Commands::WatchContest => {
            println!("Starting to watch current contest...");
            println!("Press Ctrl+C to stop watching.");
            // This also runs indefinitely - consider spawning as background task
            let cloned_client = client.clone();
            tokio::spawn(async move {
                let mut client = cloned_client.lock().await;
                if let Err(e) = client.watch_current_contest().await {
                    println!("❌ Error watching contest: {}", e);
                }
            });
            println!("✅ Contest monitoring started in background.");
        }

        Commands::GetProvers => {
            println!("Fetching provers list...");
            let cloned_client = client.clone();
            let client = cloned_client.lock().await;
            match client.get_provers().await {
                Ok(provers) => {
                    println!("📋 Registered Provers:");
                    if provers.is_empty() {
                        println!("  No provers found.");
                    } else {
                        for prover in provers.iter() {
                            println!(
                                " name: {} credits: {} no_bids: {}",
                                prover.prover_name,
                                prover.prover_credits,
                                prover.bids.len()
                            );
                        }
                    }
                }
                Err(e) => println!("❌ Failed to get provers: {}", e),
            }
        }

        Commands::Help => {
            print_help();
        }

        Commands::Quit => {
            println!("Shutting down...");
            return Ok(true); // Signal to exit
        }
    }

    Ok(false) // Continue running
}

fn print_help() {
    println!("Available commands:");
    println!("  register --name <NAME> [--team <TEAM>]  - Register a prover");
    println!("  bid --amount <AMOUNT>                   - Submit a bid");
    println!("  watch-proof                             - Start watching proof status");
    println!("  watch-contest                           - Start watching current contest");
    println!("  get-provers                             - List all registered provers");
    println!("  help                                    - Show this help message");
    println!("  quit                                    - Exit the program");
    println!();
    println!("Examples:");
    println!("  register --name alice --team Blue");
    println!("  bid --amount 1000");
    println!("  get-provers");
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Load environment variables from .env file
    dotenv::dotenv().ok();

    log_setup()?;

    let args = Args::parse();

    let client = Arc::new(Mutex::new(ProverClient::new().await?));

    println!("    ███████╗██╗   ██╗ ██████╗ ██████╗██╗███╗   ██╗ ██████╗████████╗");
    println!("    ██╔════╝██║   ██║██╔════╝██╔════╝██║████╗  ██║██╔════╝╚══██╔══╝");
    println!("    ███████╗██║   ██║██║     ██║     ██║██╔██╗ ██║██║        ██║   ");
    println!("    ╚════██║██║   ██║██║     ██║     ██║██║╚██╗██║██║        ██║   ");
    println!("    ███████║╚██████╔╝╚██████╗╚██████╗██║██║ ╚████║╚██████╗   ██║   ");
    println!("    ╚══════╝ ╚═════╝  ╚═════╝ ╚═════╝╚═╝╚═╝  ╚═══╝ ╚═════╝   ╚═╝   ");
    println!();
    println!("             🔗 Succinct Prover Client CLI started! 🔗");
    println!();

    println!("Type 'help' for available commands or 'quit' to exit.");

    // Track background tasks
    let mut background_tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    loop {
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let line = input.trim();
                if line.is_empty() {
                    continue;
                }

                // Handle special background task commands
                match line {
                    "stop-watching" => {
                        // Cancel all background tasks
                        for task in background_tasks.drain(..) {
                            task.abort();
                        }
                        println!("🛑 All background monitoring stopped.");
                        continue;
                    }
                    "status" => {
                        println!("📊 Background tasks running: {}", background_tasks.len());
                        continue;
                    }
                    _ => {}
                }

                let args: Vec<&str> = line.split_whitespace().collect();
                if args.is_empty() {
                    continue;
                }

                match CliCommand::try_parse_from(std::iter::once("").chain(args)) {
                    Ok(cli) => {
                        if handle_command_with_background(
                            cli.command,
                            client.clone(),
                            &mut background_tasks,
                        )
                        .await?
                        {
                            // Cancel all background tasks before exiting
                            for task in background_tasks {
                                task.abort();
                            }
                            break;
                        }
                    }
                    Err(e) => {
                        if line == "help" {
                            print_help_with_background();
                        } else {
                            println!("Error: {}", e);
                        }
                    }
                }
            }
            Err(error) => {
                println!("Error reading input: {}", error);
                break;
            }
        }
    }

    Ok(())
}

async fn handle_command_with_background(
    command: Commands,
    client: Arc<Mutex<ProverClient>>,
    background_tasks: &mut Vec<tokio::task::JoinHandle<()>>,
) -> Result<bool, anyhow::Error> {
    match command {
        Commands::WatchProof => {
            println!("Starting proof status monitoring in background...");
            let task = tokio::spawn(async move {
                let cloned_client = client.clone();
                let mut client = cloned_client.lock().await;
                if let Err(e) = client.watch_proof_status().await {
                    println!("❌ Proof status monitoring error: {}", e);
                }
            });
            background_tasks.push(task);
            println!("✅ Proof monitoring started. Use 'stop-watching' to stop.");
        }

        Commands::WatchContest => {
            println!("Starting contest monitoring in background...");
            let task = tokio::spawn(async move {
                let cloned_client = client.clone();
                let mut client = cloned_client.lock().await;
                if let Err(e) = client.watch_current_contest().await {
                    println!("❌ Contest monitoring error: {}", e);
                }
            });
            background_tasks.push(task);
            println!("✅ Contest monitoring started. Use 'stop-watching' to stop.");
        }

        // Handle other commands normally...
        _ => {
            // Delegate to the original handler for other commands
            return handle_command(command, client).await;
        }
    }

    Ok(false)
}

fn print_help_with_background() {
    print_help();
    println!("Additional commands:");
    println!("  stop-watching                           - Stop all background monitoring");
    println!("  status                                  - Show background task status");
}
