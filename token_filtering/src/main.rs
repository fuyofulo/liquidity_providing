use solana_program::pubkey::Pubkey;
use std::io::{self, Write};

const BASE_URL: &str = "https://api.rugcheck.xyz";

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .build()
        .expect("failed to create HTTP client")
}

async fn get_full_report(mint: &str) -> Result<serde_json::Value, reqwest::Error> {
    let url = format!("{}/v1/tokens/{}/report", BASE_URL, mint);
    let response = client()
        .get(&url)
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    Ok(response)
}

#[tokio::main]
async fn main() {
    let mut user_input = String::new();
    print!("please enter token address: ");
    io::stdout().flush().unwrap();
    io::stdin()
        .read_line(&mut user_input)
        .expect("failed to read user input");

    let mint = user_input.trim();
    let _token_address: Pubkey = mint.parse().expect("invalid pubkey");

    println!("starting rug pull check for {}", mint);

    println!("FULL REPORT");
    match get_full_report(mint).await {
        Ok(response) => {
            println!("{}", serde_json::to_string_pretty(&response).unwrap());
        }
        Err(e) => {
            eprintln!("failed to fetch full report: {}", e);
        }
    }
}
