//! Lafiya Admin CLI (Rust)
//! Reads config/networks.toml for RPC, passphrase, contract IDs.
//! Switching networks is one flag: --network testnet
//! Secrets are never read from config, only via stellar CLI identities or env.
//!
//! Every operator supplied value (network name, address, contract id, record
//! hash, admin/source account) is validated locally before the stellar CLI is
//! invoked, so malformed input fails fast with an actionable message.

use anyhow::Context;
use clap::{Parser, Subcommand};
use lafiya_config::{
    get_network, load_networks, validate_account_address, validate_address, validate_network_name,
    validate_record_hash, validate_source_account, ContractKind, DeploymentState, NetworkConfig,
};
use std::path::PathBuf;

mod deployment_ledger;
use deployment_ledger::{DeployEvent, DeploymentRecord};

/// Env var holding the stellar CLI identity used as transaction source.
const ENV_SOURCE: &str = "STELLAR_ACCOUNT";
/// Env var holding the contract admin address.
const ENV_ADMIN: &str = "ADMIN_ADDRESS";

#[derive(Parser, Debug)]
#[command(
    name = "lafiya-cli",
    about = "Lafiya Admin CLI - uses config/networks.toml"
)]
struct Cli {
    /// Network name as defined in config/networks.toml (e.g. testnet, futurenet, mainnet, local)
    #[arg(long, default_value = "testnet", global = true)]
    network: String,

    /// Path to networks.toml (auto-discovers by default)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show / list network config
    Config {
        #[command(subcommand)]
        sub: ConfigSub,
    },
    /// Attester registry operations
    Attester {
        #[command(subcommand)]
        sub: AttesterSub,
    },
    /// Attestation registry operations
    Attestation {
        #[command(subcommand)]
        sub: AttestationSub,
    },
    /// Deploy contracts (wrapper around scripts/deploy.sh logic, but uses same config)
    Deploy {
        /// Build only, don't deploy
        #[arg(long, default_value_t = false)]
        build_only: bool,
        /// Dry run
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Stellar identity or G... address used as transaction source (or STELLAR_ACCOUNT)
        #[arg(long)]
        source: Option<String>,
        /// Admin address (G...) for contract initialization (or ADMIN_ADDRESS)
        #[arg(long)]
        admin: Option<String>,
    },
    /// Append-only per-network deployment history (deployments/<network>.jsonl)
    Deployments {
        #[command(subcommand)]
        sub: DeploymentsSub,
    },
}

#[derive(Subcommand, Debug)]
enum DeploymentsSub {
    /// Append one event record to deployments/<network>.jsonl.
    ///
    /// Called automatically by scripts/deploy.sh and scripts/upgrade.sh after
    /// on-chain confirmation; can also be run by hand for an out-of-band
    /// change (e.g. admin_transfer).
    Record {
        /// deploy | initialize | upgrade | migrate | admin_transfer | repoint
        #[arg(long)]
        event: String,
        /// attester-registry | attestation-registry | multisig-account
        #[arg(long)]
        contract_kind: String,
        /// The affected contract's Stellar contract ID (C...)
        #[arg(long)]
        contract_id: String,
        /// Path to the wasm now running, to compute wasm_sha256 (omit for
        /// events that don't change code, e.g. admin_transfer)
        #[arg(long)]
        wasm: Option<PathBuf>,
        /// sha256 of the wasm this event replaced (upgrade/migrate)
        #[arg(long)]
        previous_wasm_sha256: Option<String>,
        /// Confirmed transaction hash. Omit only if it genuinely could not
        /// be captured -- the record is still appended with tx_hash: null,
        /// but `deployments verify` cannot confirm it against the chain.
        #[arg(long)]
        tx_hash: Option<String>,
        /// Ledger sequence the transaction closed in, if known
        #[arg(long)]
        ledger: Option<u32>,
        /// Identity/signer-set that authorized this event
        #[arg(long)]
        operator: String,
    },
    /// Verify a network's ledger file: hash chain intact, ledgers
    /// monotonic, required fields present. Offline check only -- does not
    /// query the chain (see deployments/README.md).
    Verify,
}

#[derive(Subcommand, Debug)]
enum ConfigSub {
    /// Show resolved config for selected network
    Show,
    /// List all available networks in config
    List,
    /// Print shell export lines for current network (for use with eval or sourcing)
    Env,
}

#[derive(Subcommand, Debug)]
enum AttesterSub {
    /// Check if an address is allowlisted
    Is {
        /// Stellar address (G...)
        address: String,
    },
    /// Add attester (requires admin - will invoke stellar CLI)
    Add {
        /// Stellar address (G...) to allowlist as an attester
        address: String,
        #[arg(long)]
        source: Option<String>,
    },
    /// Remove attester
    Remove {
        /// Stellar address (G...) to remove from the allowlist
        address: String,
        #[arg(long)]
        source: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum AttestationSub {
    /// Get attestation for a record hash (hex encoded 32-byte hash)
    Get {
        /// Hex string of 32-byte record hash (64 chars)
        record_hash: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Validate the network name before it is used as a config key.
    validate_network_name(&cli.network)
        .map_err(|e| anyhow::anyhow!("invalid --network value: {e}"))?;

    let config_path_opt = cli.config.as_deref();
    let networks = load_networks(config_path_opt)?;

    // For config list, we don't need to resolve specific network
    if let Commands::Config {
        sub: ConfigSub::List,
    } = &cli.command
    {
        println!(
            "Available networks (from {:?}):",
            lafiya_config::default_config_path()
        );
        for name in networks.keys() {
            println!("  - {}", name);
        }
        if let Some(p) = &cli.config {
            println!("Config path (explicit): {:?}", p);
        } else {
            let default = lafiya_config::default_config_path();
            println!("Config path (auto): {:?}", default);
        }
        return Ok(());
    }

    let network_cfg = get_network(&networks, &cli.network).map_err(|e| anyhow::anyhow!(e))?;

    // `config show` reports config problems instead of refusing to print, so an
    // operator can see exactly which value needs fixing. Every other command
    // requires a valid profile before touching the network.
    let is_config_show = matches!(
        cli.command,
        Commands::Config {
            sub: ConfigSub::Show
        }
    );
    if let Err(e) = network_cfg.validate(&cli.network) {
        if is_config_show {
            eprintln!("WARNING: {e}");
        } else {
            return Err(anyhow::anyhow!(e));
        }
    }

    match cli.command {
        Commands::Config { sub } => {
            match sub {
                ConfigSub::Show => {
                    let (path, _) = lafiya_config::load_network_config::<PathBuf>(
                        &cli.network,
                        cli.config.clone(),
                    )?;
                    println!("Network: {}", cli.network);
                    println!("Config: {:?}", path);
                    println!("RPC URL: {}", network_cfg.rpc_url);
                    println!("Passphrase: {}", network_cfg.network_passphrase);
                    println!(
                        "Attester registry: {}",
                        if network_cfg.contracts.attester_registry.is_empty() {
                            "<not deployed>".to_string()
                        } else {
                            network_cfg.contracts.attester_registry.clone()
                        }
                    );
                    println!(
                        "Attestation registry: {}",
                        if network_cfg.contracts.attestation_registry.is_empty() {
                            "<not deployed>".to_string()
                        } else {
                            network_cfg.contracts.attestation_registry.clone()
                        }
                    );
                    println!("Deployed: {}", network_cfg.is_deployed());
                    println!("Deployment status: {}", deployment_summary(&network_cfg));
                    println!("\nSecrets: NEVER stored in networks.toml. Use stellar identities or env vars.");
                }
                ConfigSub::List => {} // handled above
                ConfigSub::Env => {
                    println!(
                        "# Source this with: eval $(lafiya-cli --network {} config env)",
                        cli.network
                    );
                    println!("export LAFIYA_NETWORK={}", cli.network);
                    println!("export LAFIYA_RPC_URL={}", network_cfg.rpc_url);
                    println!(
                        "export LAFIYA_NETWORK_PASSPHRASE={:?}",
                        network_cfg.network_passphrase
                    );
                    println!(
                        "export LAFIYA_ATTESTER_REGISTRY_ID={}",
                        network_cfg.contracts.attester_registry
                    );
                    println!(
                        "export LAFIYA_ATTESTATION_REGISTRY_ID={}",
                        network_cfg.contracts.attestation_registry
                    );
                }
            }
        }
        Commands::Attester { sub } => match sub {
            AttesterSub::Is { address } => {
                let contract_id = network_cfg
                    .require_contract_id(&cli.network, ContractKind::AttesterRegistry)
                    .map_err(|e| anyhow::anyhow!(e))?;
                validate_address("attester address", &address)
                    .context("invalid attester address")?;

                println!("Checking is_attester for {} on {}", address, contract_id);
                println!("RPC: {}", network_cfg.rpc_url);
                let args = invoke_args(
                    &network_cfg,
                    contract_id,
                    None,
                    "is_attester",
                    &["--attester", &address],
                );
                println!("> stellar {}", args.join(" "));
                // Read-only query: report a missing/failing CLI without aborting hard.
                if which::which("stellar").is_ok() {
                    if let Err(e) = std::process::Command::new("stellar").args(args).status() {
                        eprintln!("Failed to run stellar CLI: {e}. Install with: cargo install --locked stellar-cli");
                    }
                } else {
                    eprintln!("stellar CLI not found - showing command only. Install with: cargo install --locked stellar-cli");
                }
            }
            AttesterSub::Add { address, source } => {
                let contract_id = network_cfg
                    .require_contract_id(&cli.network, ContractKind::AttesterRegistry)
                    .map_err(|e| anyhow::anyhow!(e))?;
                validate_address("attester address", &address)
                    .context("invalid attester address")?;
                let source = validated_source(source)?;

                let args = invoke_args(
                    &network_cfg,
                    contract_id,
                    source.as_deref(),
                    "add_attester",
                    &["--attester", &address],
                );
                run_stellar(args)?;
            }
            AttesterSub::Remove { address, source } => {
                let contract_id = network_cfg
                    .require_contract_id(&cli.network, ContractKind::AttesterRegistry)
                    .map_err(|e| anyhow::anyhow!(e))?;
                validate_address("attester address", &address)
                    .context("invalid attester address")?;
                let source = validated_source(source)?;

                let args = invoke_args(
                    &network_cfg,
                    contract_id,
                    source.as_deref(),
                    "remove_attester",
                    &["--attester", &address],
                );
                run_stellar(args)?;
            }
        },
        Commands::Attestation { sub } => match sub {
            AttestationSub::Get { record_hash } => {
                let contract_id = network_cfg
                    .require_contract_id(&cli.network, ContractKind::AttestationRegistry)
                    .map_err(|e| anyhow::anyhow!(e))?;
                validate_record_hash("record_hash", &record_hash)
                    .context("invalid record hash (expected a hex encoded 32-byte hash)")?;

                let args = invoke_args(
                    &network_cfg,
                    contract_id,
                    None,
                    "get_attestation",
                    &["--record_hash", &record_hash],
                );
                println!("> stellar {}", args.join(" "));
                if which::which("stellar").is_ok() {
                    let status = std::process::Command::new("stellar").args(args).status()?;
                    if !status.success() {
                        anyhow::bail!("stellar CLI failed");
                    }
                } else {
                    eprintln!(
                        "stellar CLI not found - install with cargo install --locked stellar-cli"
                    );
                }
            }
        },
        Commands::Deploy {
            build_only,
            dry_run,
            source,
            admin,
        } => {
            let identity = DeployIdentity::resolve(
                admin,
                source,
                std::env::var(ENV_ADMIN).ok(),
                std::env::var(ENV_SOURCE).ok(),
                DeployMode::new(build_only, dry_run),
            )?;

            println!("Deploy flow for network: {}", cli.network);
            println!("RPC: {}", network_cfg.rpc_url);
            println!("Passphrase: {}", network_cfg.network_passphrase);
            println!("Current deployment: {}", deployment_summary(&network_cfg));
            println!("Source: {}", identity.source.as_deref().unwrap_or("<none>"));
            println!("Admin: {}", identity.admin.as_deref().unwrap_or("<none>"));
            println!("This command is a wrapper- for full deploy use:");
            println!("  ./scripts/deploy.sh --network {}", cli.network);
            if build_only {
                println!("Building WASM...");
                let status = std::process::Command::new("cargo")
                    .args([
                        "build",
                        "--workspace",
                        "--release",
                        "--target",
                        "wasm32v1-none",
                    ])
                    .status()?;
                if !status.success() {
                    anyhow::bail!("build failed");
                }
            }
            if dry_run {
                println!(
                    "[dry-run] Would deploy attester-registry and attestation-registry to {}",
                    cli.network
                );
            }
        }
        Commands::Deployments { sub } => match sub {
            DeploymentsSub::Record {
                event,
                contract_kind,
                contract_id,
                wasm,
                previous_wasm_sha256,
                tx_hash,
                ledger,
                operator,
            } => {
                let event: DeployEvent = event
                    .parse()
                    .map_err(|e: String| anyhow::anyhow!(e))?;
                let wasm_sha256 = wasm
                    .as_deref()
                    .map(wasm_sha256_hex)
                    .transpose()
                    .context("failed to hash --wasm")?;
                if tx_hash.is_none() {
                    eprintln!(
                        "WARNING: no --tx-hash supplied; recording with tx_hash: null. \
                         `deployments verify` cannot confirm this record against the chain."
                    );
                }
                let record = DeploymentRecord {
                    event,
                    contract_kind,
                    contract_id,
                    wasm_sha256,
                    previous_wasm_sha256,
                    tx_hash,
                    ledger,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    git_commit: git_commit_hash().unwrap_or_else(|_| "unknown".to_string()),
                    release_version: env!("CARGO_PKG_VERSION").to_string(),
                    operator,
                    prev_record_sha256: None,
                };
                let dir = deployments_dir();
                let path = deployment_ledger::append(&dir, &cli.network, record)
                    .map_err(|e| anyhow::anyhow!(e))?;
                println!("Appended deployment record to {}", path.display());
            }
            DeploymentsSub::Verify => {
                let dir = deployments_dir();
                let report = deployment_ledger::verify(&dir, &cli.network)
                    .map_err(|e| anyhow::anyhow!(e))?;
                println!(
                    "Checked {} record(s) for network {}",
                    report.records_checked, cli.network
                );
                if report.is_ok() {
                    println!("OK: ledger is internally consistent.");
                } else {
                    for err in &report.errors {
                        eprintln!("ERROR: {err}");
                    }
                    anyhow::bail!("{} error(s) found in deployment ledger", report.errors.len());
                }
            }
        },
    }

    Ok(())
}

/// `<repo-root>/deployments`, found the same way `lafiya_config::default_config_path`
/// locates `config/networks.toml`: relative to the current working directory.
fn deployments_dir() -> PathBuf {
    PathBuf::from("deployments")
}

fn wasm_sha256_hex(path: &std::path::Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).with_context(|| format!("reading {path:?}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn git_commit_hash() -> anyhow::Result<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .context("running git rev-parse HEAD")?;
    if !out.status.success() {
        anyhow::bail!("git rev-parse HEAD failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

mod which {
    use std::path::Path;

    pub fn which(bin: &str) -> Result<std::path::PathBuf, ()> {
        // Simple check using PATH env
        if let Some(paths) = std::env::var_os("PATH") {
            for p in std::env::split_paths(&paths) {
                let full = p.join(bin);
                if full.exists() {
                    return Ok(full);
                }
                // Windows also .exe etc, but we target unix for stellar
                #[cfg(windows)]
                {
                    let full_exe = p.join(format!("{}.exe", bin));
                    if full_exe.exists() {
                        return Ok(full_exe);
                    }
                }
                // Also check without extension but with executable bit
                if Path::new(&full).exists() {
                    return Ok(full);
                }
            }
        }
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `attester add` requires a positional `address`. Clap's derive-generated
    // error for a missing required argument must still name it, so a
    // contributor testing the CLI by hand isn't left guessing which value
    // they forgot.
    #[test]
    fn attester_add_missing_address_names_the_argument() {
        let err = Cli::try_parse_from(["lafiya-cli", "attester", "add"])
            .expect_err("expected a missing required argument error");
        let message = err.to_string();
        assert!(
            message.to_uppercase().contains("ADDRESS"),
            "expected error to name the missing `address` argument, got: {message}"
        );
    }

    // `attestation get` requires a positional `record_hash`.
    #[test]
    fn attestation_get_missing_record_hash_names_the_argument() {
        let err = Cli::try_parse_from(["lafiya-cli", "attestation", "get"])
            .expect_err("expected a missing required argument error");
        let message = err.to_string();
        assert!(
            message.to_uppercase().contains("RECORD_HASH"),
            "expected error to name the missing `record_hash` argument, got: {message}"
        );
    }
}
