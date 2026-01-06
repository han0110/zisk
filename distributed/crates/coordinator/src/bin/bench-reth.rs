use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::post,
    Router,
};
use clap::Parser;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tonic::transport::Channel;
use tracing::{error, info};
use zisk_distributed_common::dto::WebhookPayloadDto;
use zisk_distributed_grpc_api::{
    launch_proof_response, zisk_distributed_api_client::ZiskDistributedApiClient, InputMode,
    LaunchProofRequest,
};

#[derive(Parser)]
struct Args {
    #[arg(short, long, default_value_t = 10)]
    compute_capacity: u32,
}

type JobChannel = Arc<Mutex<HashMap<String, oneshot::Sender<WebhookPayloadDto>>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    let job_channel: JobChannel = Arc::new(Mutex::new(HashMap::new()));

    {
        let job_channel = job_channel.clone();

        tokio::spawn(async move {
            let app =
                Router::new().route("/:job_id", post(webhook_handler)).with_state(job_channel);

            let listener = tokio::net::TcpListener::bind("127.0.0.1:50052")
                .await
                .expect("Failed to bind to port 50052");

            info!("Starting webhook server listening for webhooks at: POST http://localhost:50052/{{job_id}}");

            axum::serve(listener, app).await.expect("Failed to start webhook server");
        });
    }

    let coordinator_url = "http://localhost:50051";
    let channel = Channel::from_shared(coordinator_url)?.connect().await?;
    let client = ZiskDistributedApiClient::new(channel);

    for suffix in 0..100 {
        let block_number = format!("241726{suffix:02}");
        let input_data =
            tokio::fs::read(format!("./block/rpc_block_{block_number}")).await.unwrap();

        let launch_proof_request = LaunchProofRequest {
            data_id: block_number.clone(),
            compute_capacity: args.compute_capacity,
            input_mode: InputMode::Data.into(),
            input_path: None,
            input_data: Some(input_data),
            simulated_node: None,
        };

        info!("Sending request for block {block_number}");
        let response = client.clone().launch_proof(launch_proof_request).await?;

        let job_id = match response.into_inner().result {
            Some(launch_proof_response::Result::JobId(job_id)) => job_id,
            Some(launch_proof_response::Result::Error(error)) => {
                info!("Proof job failed: {} - {}", error.code, error.message);
                anyhow::bail!("Job launch failed");
            }
            None => {
                info!("Received empty response from coordinator");
                anyhow::bail!("Empty response");
            }
        };

        let (tx, rx) = oneshot::channel::<WebhookPayloadDto>();
        job_channel.lock().await.insert(job_id.clone(), tx);

        info!("Waiting for webhook for job_id {job_id}");
        match rx.await {
            Ok(payload) => {
                info!("Proof created for block {block_number}: {} ms", payload.duration_ms);

                if let Some(error) = &payload.error {
                    error!("  Error code: {}", error.code);
                    error!("  Error message: {}", error.message);
                }
            }
            Err(_) => {
                info!("Failed to receive webhook response");
            }
        }
    }

    Ok(())
}

async fn webhook_handler(
    Path(job_id): Path<String>,
    State(job_channel): State<JobChannel>,
    Json(payload): Json<WebhookPayloadDto>,
) -> Result<(), StatusCode> {
    info!("Received webhook for job_id {}", job_id);

    let mut channels = job_channel.lock().await;
    if let Some(tx) = channels.remove(&job_id) {
        match tx.send(payload) {
            Ok(_) => info!("Sent payload for job {job_id}"),
            Err(_) => error!("Failed to send payload for job {job_id}"),
        }
    } else {
        error!("Unknown job_id {}", job_id);
    }
    Ok(())
}
