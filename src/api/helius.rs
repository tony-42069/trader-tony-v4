use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info};

use crate::models::token::TokenMetadata;

const HELIUS_BASE_URL: &str = "https://api.helius.xyz";

#[derive(Debug, Clone)]
pub struct HeliusClient {
    api_key: String,
    client: Client,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasAsset {
    pub id: String,              // Token mint address
    pub content: Option<DasAssetContent>,
    pub authorities: Vec<DasAuthority>,
    pub compression: DasCompression,
    pub grouping: Vec<DasGrouping>,
    pub royalty: DasFees,
    pub ownership: DasOwnership,
    pub creators: Vec<DasCreator>,
    pub uses: Option<DasUses>,
    pub supply: Option<DasSupply>,
    pub interface: String,
    pub mutable: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasAssetContent {
    #[serde(rename = "$schema")]
    pub schema: Option<String>,
    pub json_uri: Option<String>,
    pub files: Option<Vec<DasFile>>,
    pub metadata: Option<DasMetadata>,
    pub links: Option<DasLinks>,
}

// Additional structs for DasAsset components...
#[derive(Debug, Deserialize, Serialize)]
pub struct DasAuthority {
    pub address: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasCompression {
    pub eligible: bool,
    pub compressed: bool,
    pub data_hash: Option<String>,
    pub creator_hash: Option<String>,
    pub asset_hash: Option<String>,
    pub tree: Option<String>,
    pub seq: Option<i64>,
    pub leaf_id: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasGrouping {
    pub group_key: String,
    pub group_value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasFees {
    pub basis_points: i64,
    pub primary_sale_happened: bool,
    pub locked: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasOwnership {
    pub owner: Option<String>,
    pub delegated: bool,
    pub delegate: Option<String>,
    pub ownership_model: String,
    pub frozen: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasCreator {
    pub address: String,
    pub share: i64,
    pub verified: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasUses {
    pub use_method: String,
    pub remaining: i64,
    pub total: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasSupply {
    pub print_max_supply: i64,
    pub print_current_supply: i64,
    pub edition_nonce: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasFile {
    pub uri: Option<String>,
    pub mime: Option<String>,
    pub cdn_uri: Option<String>,
    pub quality: Option<String>,
    pub contexts: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub attributes: Option<Vec<DasAttribute>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasAttribute {
    pub trait_type: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DasLinks {
    pub image: Option<String>,
    pub animation: Option<String>,
    pub external_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(non_snake_case)] // Allow non-snake-case fields for this struct mapping to API
pub struct SearchAssetsRequest {
    pub ownerAddress: Option<String>,
    pub creatorAddress: Option<String>,
    pub limit: Option<u32>,
    pub page: Option<u32>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub sortBy: Option<String>,
    pub sortDirection: Option<String>,
    pub burnt: Option<bool>,
    pub delegate: Option<String>,
    pub frozen: Option<bool>,
    pub supplyMint: Option<String>,
    pub grouping: Option<Vec<DasGrouping>>,
    pub groupValue: Option<String>,
    pub compressed: Option<bool>,
    pub compressible: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SearchAssetsResponse {
    pub items: Vec<DasAsset>,
    pub total: u32,
    pub limit: u32,
    pub page: u32,
    pub before: Option<String>,
    pub after: Option<String>,
}

impl HeliusClient {
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            client: Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }
    
    pub async fn search_assets(&self, owner_address: Option<&str>, limit: Option<u32>) -> Result<Vec<DasAsset>> {
        // Corrected URL construction: Use v0 for DAS API, not v1
        let url = format!("{}/v0/das/searchAssets?api-key={}", HELIUS_BASE_URL, self.api_key);
        
        let request = SearchAssetsRequest {
            ownerAddress: owner_address.map(String::from),
            creatorAddress: None,
            limit: Some(limit.unwrap_or(100)),
            page: None, // Using page 1 explicitly might be better than None
            before: None,
            after: None,
            sortBy: Some("created".to_string()), // Ensure this field is valid for DAS API
            sortDirection: Some("desc".to_string()),
            burnt: Some(false),
            delegate: None,
            frozen: None,
            supplyMint: None,
            grouping: None,
            groupValue: None,
            compressed: None,
            compressible: None,
        };
        
        debug!("Searching for assets with Helius DAS: {:?}", request);
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Helius DAS API")?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Helius DAS API error: {} - {}", status, error_text);
            anyhow::bail!("Helius DAS API error: {} - {}", status, error_text);
        }
        
        // Changed response type to match the expected structure from Helius DAS API
        #[derive(Debug, Deserialize)]
        struct HeliusSearchResponse {
            result: SearchAssetsResponse,
        }

        let search_response_wrapper: HeliusSearchResponse = response
            .json()
            .await
            .context("Failed to parse Helius DAS API response")?;
        
        let search_response = search_response_wrapper.result;

        debug!("Found {} assets via Helius DAS", search_response.items.len());
        
        Ok(search_response.items)
    }
    
    // This function needs significant refinement based on how Helius DAS actually returns token creation data.
    // The current implementation makes assumptions that might not hold.
    pub async fn get_recent_tokens(&self, _max_age_minutes: u64) -> Result<Vec<TokenMetadata>> {
        // The concept of searching by owner_address = TokenProgram might not be the right way
        // to find *newly created* tokens with DAS. You might need a different approach,
        // perhaps querying recent transactions or using a dedicated Helius endpoint if available.
        // For now, this is a placeholder based on the provided code.
        
        // Using a known program like TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA as owner might not yield SPL tokens.
        // It might be better to search without an owner or use a different filter if the goal is new SPL tokens.
        // let token_program = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
        
        info!("[Helius DAS] Scanning for new tokens (placeholder implementation)...");
        
        // Searching without owner might return too many results or irrelevant assets.
        // Limit is kept small for demonstration.
        let assets = self.search_assets(None, Some(50)).await?;
        
        info!("[Helius DAS] Received {} assets. Filtering (basic)...", assets.len());
        
        let tokens: Vec<TokenMetadata> = assets // Add explicit type annotation
            .into_iter()
            .filter_map(|asset| {
                // Basic filtering: Check if it's likely an SPL token (e.g., Fungible interface)
                // and has some metadata. This is highly speculative without real API data.
                if asset.interface != "V1_NFT" && asset.content.is_some() { // Example: Filter out NFTs
                    let token_address = asset.id.clone();
                    let metadata = asset.content.as_ref()?.metadata.as_ref()?;
                    
                    let token_name = metadata.name.clone().unwrap_or_else(|| "Unknown".to_string());
                    let token_symbol = metadata.symbol.clone().unwrap_or_else(|| "UNK".to_string());
                    
                    // TODO: Extract decimals, supply, creation time accurately from DAS data if available.
                    // The structure provided doesn't clearly show where these would be for SPL tokens.
                    // Helius might have specific fields or require fetching token accounts separately.
                    
                    debug!("[Helius DAS] Potential token found: {} ({}) - Addr: {}", token_name, token_symbol, token_address);
                    
                    Some(TokenMetadata {
                        address: token_address,
                        name: token_name,
                        symbol: token_symbol,
                        decimals: 9, // Placeholder: Needs actual data
                        supply: asset.supply.map(|s| s.print_current_supply as u64), // Placeholder
                        logo_uri: asset.content.as_ref()?.links.as_ref()?.image.clone(), // Placeholder
                        creation_time: None, // Placeholder: Needs actual data
                    })
                } else {
                    None
                }
            })
            .collect();
        
        info!("[Helius DAS] Filtered down to {} potential tokens.", tokens.len());
        
        Ok(tokens)
    }
    
    // TODO: Implement methods for:
    // - Getting detailed token metadata (getAsset endpoint)
    // - Performing security checks (requires specific Helius endpoints or logic)
}
