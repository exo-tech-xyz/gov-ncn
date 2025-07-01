use std::fmt;

use clap::{Parser, Subcommand, ValueEnum};
use solana_sdk::clock::DEFAULT_SLOTS_PER_EPOCH;

fn parse_hex_32(s: &str) -> Result<[u8; 32], String> {
    let bytes = hex::decode(s).map_err(|e| format!("Invalid hex: {}", e))?;
    if bytes.len() != 32 {
        return Err("Expected 64 hex characters (32 bytes)".into());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

#[derive(Parser)]
#[command(author, version, about = "A CLI for creating and managing the ncn program", long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: ProgramCommand,

    #[arg(
        long,
        global = true,
        env = "RPC_URL",
        default_value = "https://api.mainnet-beta.solana.com",
        help = "RPC URL to use"
    )]
    pub rpc_url: String,

    #[arg(
        long,
        global = true,
        env = "COMMITMENT",
        default_value = "confirmed",
        help = "Commitment level"
    )]
    pub commitment: String,

    #[arg(
        long,
        global = true,
        env = "PRIORITY_FEE_MICRO_LAMPORTS",
        default_value_t = 1,
        help = "Priority fee in micro lamports"
    )]
    pub priority_fee_micro_lamports: u64,

    #[arg(
        long,
        global = true,
        env = "TRANSACTION_RETRIES",
        default_value_t = 0,
        help = "Amount of times to retry a transaction"
    )]
    pub transaction_retries: u64,

    #[arg(
        long,
        global = true,
        env = "NCN_PROGRAM_ID",
        default_value_t = ncn_program::id().to_string(),
        help = "NCN program ID"
    )]
    pub ncn_program_id: String,

    #[arg(
        long,
        global = true,
        env = "RESTAKING_PROGRAM_ID",
        default_value_t = jito_restaking_program::id().to_string(),
        help = "Restaking program ID"
    )]
    pub restaking_program_id: String,

    #[arg(
        long,
        global = true,
        env = "VAULT_PROGRAM_ID", 
        default_value_t = jito_vault_program::id().to_string(),
        help = "Vault program ID"
    )]
    pub vault_program_id: String,

    #[arg(
        long,
        global = true,
        env = "TOKEN_PROGRAM_ID",
        default_value_t = spl_token::id().to_string(),
        help = "Token Program ID"
    )]
    pub token_program_id: String,

    #[arg(long, global = true, env = "NCN", help = "NCN Account Address")]
    pub ncn: Option<String>,

    #[arg(
        long,
        global = true,
        env = "EPOCH",
        help = "Epoch - defaults to current epoch"
    )]
    pub epoch: Option<u64>,

    #[arg(long, global = true, env = "KEYPAIR_PATH", help = "keypair path")]
    pub keypair_path: Option<String>,

    #[arg(long, global = true, help = "Verbose mode")]
    pub verbose: bool,

    #[arg(long, global = true, hide = true)]
    pub markdown_help: bool,

    #[arg(
        long,
        global = true,
        env = "OPENWEATHER_API_KEY",
        help = "Open weather api key"
    )]
    pub open_weather_api_key: Option<String>,
}

#[derive(Subcommand)]
pub enum ProgramCommand {
    /// NCN Keeper
    RunKeeper {
        #[arg(
            long,
            env,
            default_value_t = 600_000, // 10 minutes
            help = "Maximum time in milliseconds between keeper loop iterations"
        )]
        loop_timeout_ms: u64,
        #[arg(
            long,
            env,
            default_value_t = 10_000, // 10 seconds
            help = "Timeout in milliseconds when an error occurs before retrying"
        )]
        error_timeout_ms: u64,
    },

    /// Operator Keeper
    RunOperator {
        #[arg(long, help = "Operator address")]
        operator: String,
        #[arg(
            long,
            env,
            default_value_t = 600_000, // 10 minutes
            help = "Maximum time in milliseconds between keeper loop iterations"
        )]
        loop_timeout_ms: u64,
        #[arg(
            long,
            env,
            default_value_t = 10_000, // 10 seconds
            help = "Timeout in milliseconds when an error occurs before retrying"
        )]
        error_timeout_ms: u64,
    },
    /// Crank Functions
    CrankUpdateAllVaults {},
    CrankRegisterVaults {},
    CrankSnapshot {},
    CrankDistribute {},
    CrankCloseEpochAccounts {},
    SetEpochWeights {},

    /// Admin
    AdminCreateConfig {
        #[arg(long, help = "Ncn Fee Wallet Address")]
        ncn_fee_wallet: String,
        #[arg(long, help = "Ncn Fee bps")]
        ncn_fee_bps: u64,

        #[arg(long, default_value_t = 10 as u64, help = "Epochs before tie breaker can set consensus")]
        epochs_before_stall: u64,
        #[arg(long, default_value_t = (DEFAULT_SLOTS_PER_EPOCH as f64 * 0.1) as u64, help = "Valid slots after consensus")]
        valid_slots_after_consensus: u64,
        #[arg(
            long,
            default_value_t = 10,
            help = "Epochs after consensus before accounts can be closed"
        )]
        epochs_after_consensus_before_close: u64,
        #[arg(long, help = "Tie breaker admin address")]
        tie_breaker_admin: Option<String>,
    },
    AdminRegisterStMint {
        #[arg(long, help = "Vault address")]
        vault: String,
        #[arg(long, help = "Weight")]
        weight: Option<u128>,
    },

    AdminSetWeight {
        #[arg(long, help = "Vault address")]
        vault: String,
        #[arg(long, help = "Weight value")]
        weight: u128,
    },
    AdminSetTieBreaker {
        #[arg(long, value_parser = parse_hex_32, help = "Merkle Root of MetaMerkleTree as 64-char hex")]
        merkle_root: [u8; 32],
        #[arg(long, value_parser = parse_hex_32, help = "Hash of the Snapshot JSON as 64-char hex")]
        snapshot_hash: [u8; 32],
    },
    AdminSetParameters {
        #[arg(long, help = "Epochs before tie breaker can set consensus")]
        epochs_before_stall: Option<u64>,
        #[arg(long, help = "Epochs after consensus before accounts can be closed")]
        epochs_after_consensus_before_close: Option<u64>,
        #[arg(long, help = "Slots to which voting is allowed after consensus")]
        valid_slots_after_consensus: Option<u64>,
        #[arg(long, help = "Starting valid epoch")]
        starting_valid_epoch: Option<u64>,
    },
    AdminSetNewAdmin {
        #[arg(long, help = "New admin address")]
        new_admin: String,
        #[arg(long, help = "Set tie breaker admin")]
        set_tie_breaker_admin: bool,
    },
    AdminFundAccountPayer {
        #[arg(long, help = "Amount of SOL to fund")]
        amount_in_sol: f64,
    },

    /// Instructions
    CreateVaultRegistry,

    RegisterVault {
        #[arg(long, help = "Vault address")]
        vault: String,
    },

    CreateEpochState,

    CreateWeightTable,

    CreateEpochSnapshot,

    CreateOperatorSnapshot {
        #[arg(long, help = "Operator address")]
        operator: String,
    },

    SnapshotVaultOperatorDelegation {
        #[arg(long, help = "Vault address")]
        vault: String,
        #[arg(long, help = "Operator address")]
        operator: String,
    },

    CreateBallotBox,

    OperatorCastVote {
        #[arg(long, help = "Operator address")]
        operator: String,
        #[arg(long, value_parser = parse_hex_32, help = "Merkle Root of MetaMerkleTree as 64-char hex")]
        merkle_root: [u8; 32],
        #[arg(long, value_parser = parse_hex_32, help = "Hash of the Snapshot JSON as 64-char hex")]
        snapshot_hash: [u8; 32],
    },

    CreateNCNRewardRouter,

    CreateOperatorVaultRewardRouter {
        #[arg(long, help = "Operator address")]
        operator: String,
    },

    RouteNCNRewards,

    RouteOperatorVaultRewards {
        #[arg(long, help = "Operator address")]
        operator: String,
    },

    DistributeBaseOperatorVaultRewards {
        #[arg(long, help = "Operator address")]
        operator: String,
    },

    /// Getters
    GetNcn,
    GetNcnOperatorState {
        #[arg(long, env = "OPERATOR", help = "Operator Account Address")]
        operator: String,
    },
    GetVaultNcnTicket {
        #[arg(long, env = "VAULT", help = "Vault Account Address")]
        vault: String,
    },
    GetNcnVaultTicket {
        #[arg(long, env = "VAULT", help = "Vault Account Address")]
        vault: String,
    },
    GetVaultOperatorDelegation {
        #[arg(long, env = "VAULT", help = "Vault Account Address")]
        vault: String,
        #[arg(long, env = "OPERATOR", help = "Operator Account Address")]
        operator: String,
    },
    GetAllTickets,
    GetAllOperatorsInNcn,
    GetAllVaultsInNcn,
    GetNCNProgramConfig,
    GetVaultRegistry,
    GetWeightTable,
    GetEpochState,
    GetEpochSnapshot,
    GetOperatorSnapshot {
        #[arg(long, env = "OPERATOR", help = "Operator Account Address")]
        operator: String,
    },
    GetBallotBox,
    GetAccountPayer,
    GetTotalEpochRentCost,
    GetConsensusResult,

    GetOperatorStakes,
    GetVaultStakes,
    GetVaultOperatorStakes,

    GetNCNRewardRouter,
    GetNCNRewardReceiverAddress,
    GetOperatorVaultRewardRouter {
        #[arg(long, env = "OPERATOR", help = "Operator Account Address")]
        operator: String,
    },
    GetAllOperatorVaultRewardRouters,

    // GetAllOptedInValidators,
    FullUpdateVaults {
        #[arg(long, help = "Vault address")]
        vault: Option<String>,
    },
}

#[rustfmt::skip]
impl fmt::Display for Args {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n NCN Program CLI Configuration")?;
        writeln!(f, "═══════════════════════════════════════")?;

        // Network Configuration
        writeln!(f, "\n📡 Network Settings:")?;
        writeln!(f, "  • RPC URL:     {}", self.rpc_url)?;
        writeln!(f, "  • Commitment:  {}", self.commitment)?;

        // Program IDs
        writeln!(f, "\n🔑 Program IDs:")?;
        writeln!(f, "  • NCN Program:        {}", self.ncn_program_id)?;
        writeln!(f, "  • Restaking:         {}", self.restaking_program_id)?;
        writeln!(f, "  • Vault:             {}", self.vault_program_id)?;
        writeln!(f, "  • Token:             {}", self.token_program_id)?;

        // Solana Settings
        writeln!(f, "\n◎  Solana Settings:")?;
        writeln!(f, "  • Keypair Path:  {}", self.keypair_path.as_deref().unwrap_or("Not Set"))?;
        writeln!(f, "  • NCN:  {}", self.ncn.as_deref().unwrap_or("Not Set"))?;
        writeln!(f, "  • Epoch: {}", if self.epoch.is_some() { format!("{}", self.epoch.unwrap()) } else { "Current".to_string() })?;

        // Optional Settings
        writeln!(f, "\n⚙️  Additional Settings:")?;
        writeln!(f, "  • Verbose Mode:  {}", if self.verbose { "Enabled" } else { "Disabled" })?;
        writeln!(f, "  • Markdown Help: {}", if self.markdown_help { "Enabled" } else { "Disabled" })?;

        writeln!(f, "\n")?;

        Ok(())
    }
}

#[derive(ValueEnum, Debug, Clone)]
pub enum Cluster {
    Mainnet,
    Testnet,
    Localnet,
}

impl fmt::Display for Cluster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mainnet => write!(f, "mainnet"),
            Self::Testnet => write!(f, "testnet"),
            Self::Localnet => write!(f, "localnet"),
        }
    }
}
