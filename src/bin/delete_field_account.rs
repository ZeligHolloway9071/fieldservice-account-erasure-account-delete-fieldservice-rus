use fieldservice_account_erasure::{
    account_erasure::{erase_account, AccountDeletion},
    infrai_client::InfraiClient,
};
use std::{env, fs, process::ExitCode};

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let input_path = env::args()
        .nth(1)
        .ok_or("usage: delete_field_account <request.json>")?;
    let request: AccountDeletion = serde_json::from_str(&fs::read_to_string(input_path)?)?;
    let receipt = erase_account(&InfraiClient::from_env()?, request).await?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
