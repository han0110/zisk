use anyhow::Result;
use clap::Parser;
use futures_util::StreamExt;
use std::path::PathBuf;
use std::process::Command;
use tonic::transport::Channel;
use tracing::{error, info};
use zisk_distributed_grpc_api::{
    zisk_distributed_api_client::ZiskDistributedApiClient, InputMode, LaunchProofRequest,
    ProofStatusType, SubscribeToProofRequest,
};

#[derive(Parser)]
struct Args {
    #[arg(short, long, default_value_t = 10)]
    compute_capacity: u32,
    #[arg(short, long)]
    input: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let coordinator_url =
        std::env::var("COORDINATOR_URL").unwrap_or_else(|_| "http://127.0.0.1:50051".to_string());

    info!("Reading input from: {}", args.input.display());
    let input_data = tokio::fs::read(&args.input).await?;

    info!("Connecting to coordinator at {}", coordinator_url);
    let channel = Channel::from_shared(coordinator_url.clone())?.connect().await?;

    let mut client = ZiskDistributedApiClient::new(channel);

    info!("Launching proof request with compute_capacity {}", args.compute_capacity);

    let launch_request = LaunchProofRequest {
        data_id: args.input.to_string_lossy().to_string(),
        compute_capacity: args.compute_capacity,
        input_mode: InputMode::Data.into(),
        input_path: None,
        input_data: Some(input_data.clone()),
        simulated_node: None,
    };

    let launch_response = client.launch_proof(launch_request).await?;

    let job_id = match launch_response.into_inner().result {
        Some(zisk_distributed_grpc_api::launch_proof_response::Result::JobId(job_id)) => {
            info!("Proof job launched successfully with job_id: {}", job_id);
            job_id
        }
        Some(zisk_distributed_grpc_api::launch_proof_response::Result::Error(error)) => {
            anyhow::bail!("Proof launch failed: {}", error.message);
        }
        None => {
            anyhow::bail!("Empty response from coordinator");
        }
    };

    info!("Subscribing to job completion for job_id: {}", job_id);

    let subscribe_request = SubscribeToProofRequest { job_id: job_id.clone() };

    let mut stream = client.subscribe_to_proof(subscribe_request).await?.into_inner();

    info!("Waiting for proof completion...");

    if let Some(update_result) = stream.next().await {
        match update_result {
            Ok(update) => {
                info!("Received proof status update for job: {}", update.job_id);

                match ProofStatusType::try_from(update.status) {
                    Ok(ProofStatusType::ProofStatusCompleted) => {
                        info!("Proof completed successfully!");
                        info!("Duration: {} ms", update.duration_ms);

                        if let Some(final_proof) = update.final_proof {
                            let proof_dir = PathBuf::from("./proofs");
                            std::fs::create_dir_all(&proof_dir)?;

                            zisk_common::save_proof(
                                job_id.as_str(),
                                proof_dir.clone(),
                                &final_proof.values,
                                false,
                            )?;

                            let proof_file = proof_dir.join(format!("proof_{}.fri", job_id));
                            info!("Proof saved to: {}", proof_file.display());

                            info!("Running cargo-zisk verify...");
                            let verify_output = Command::new("cargo-zisk")
                                .arg("verify")
                                .arg("-p")
                                .arg(&proof_file)
                                .output()?;

                            if verify_output.status.success() {
                                info!("Proof verification with cargo-zisk PASSED!");
                                return Ok(());
                            } else {
                                anyhow::bail!("Proof verification with cargo-zisk FAILED!");
                            }
                        } else {
                            anyhow::bail!("No proof data in completion message");
                        }
                    }
                    Ok(ProofStatusType::ProofStatusFailed) => {
                        error!("Proof job failed!");

                        if let Some(error) = update.error {
                            anyhow::bail!("Proof job failed: {}", error.message);
                        } else {
                            anyhow::bail!("Proof job failed with unknown error");
                        }
                    }
                    Err(_) => {
                        anyhow::bail!("Unknown proof status");
                    }
                }
            }
            Err(status) => {
                anyhow::bail!("Subscription stream error: {}", status);
            }
        }
    }

    anyhow::bail!("Unexpected stream termination");
}
