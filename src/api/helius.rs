use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error, info};

use crate::models::token::TokenMetadata;

const HELIUS_RPC_URL: &str = "https://mainnet.helius-rpc.com";

#[derive(Debug, Clone)]
pub struct HeliusClient {
    api_key: String,
    client: Client,
}

/// JSON-RPC request wrapper for Helius DAS API
#[derive(Debug, Serialize)]
struct JsonRpcRequest<T> {
    jsonrpc: &'static str,
    id: &'static str,
    method: &'static str,
    params: T,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownerAddress: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creatorAddress: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sortBy: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sortDirection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burnt: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delegate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frozen: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supplyMint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grouping: Option<Vec<DasGrouping>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub groupValue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
        // Use JSON-RPC format for Helius DAS API
        let url = format!("{}/?api-key={}", HELIUS_RPC_URL, self.api_key);

        let params = SearchAssetsRequest {
            ownerAddress: owner_address.map(String::from),
            creatorAddress: None,
            limit: Some(limit.unwrap_or(100)),
            page: Some(1),
            before: None,
            after: None,
            sortBy: Some(serde_json::json!({"sortBy": "created", "sortDirection": "desc"})),
            sortDirection: None,
            burnt: Some(false),
            delegate: None,
            frozen: None,
            supplyMint: None,
            grouping: None,
            groupValue: None,
            compressed: None,
            compressible: None,
        };

        let rpc_request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: "helius-search",
            method: "searchAssets",
            params: &params,
        };

        debug!("Searching for assets with Helius DAS (JSON-RPC): {:?}", params);

        let response = self.client
            .post(&url)
            .json(&rpc_request)
            .send()
            .await
            .context("Failed to send request to Helius DAS API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Helius DAS API error: {} - {}", status, error_text);
            anyhow::bail!("Helius DAS API error: {} - {}", status, error_text);
        }

        // JSON-RPC response format
        #[derive(Debug, Deserialize)]
        struct JsonRpcResponse {
            result: SearchAssetsResponse,
        }

        let rpc_response: JsonRpcResponse = response
            .json()
            .await
            .context("Failed to parse Helius DAS API response")?;

        let search_response = rpc_response.result;

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
    
    /// Gets detailed token metadata for a specific token address
    pub async fn get_token_metadata(&self, token_address: &str) -> Result<TokenMetadata> {
        // Use JSON-RPC format for Helius DAS API
        let url = format!("{}/?api-key={}", HELIUS_RPC_URL, self.api_key);

        debug!("Fetching token metadata for: {}", token_address);

        #[derive(Serialize)]
        struct GetAssetParams {
            id: String,
        }

        let rpc_request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: "helius-get-asset",
            method: "getAsset",
            params: GetAssetParams {
                id: token_address.to_string(),
            },
        };

        let response = self.client
            .post(&url)
            .json(&rpc_request)
            .send()
            .await
            .context("Failed to send request to Helius getAsset API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("Helius getAsset API error: {} - {}", status, error_text);
            anyhow::bail!("Helius getAsset API error: {} - {}", status, error_text);
        }

        #[derive(Debug, Deserialize)]
        struct JsonRpcAssetResponse {
            result: DasAsset,
        }

        let asset_response_wrapper: JsonRpcAssetResponse = response
            .json()
            .await
            .context("Failed to parse Helius getAsset API response")?;

        let asset = asset_response_wrapper.result;

        // Convert DAS asset to TokenMetadata
        let metadata = asset.content.as_ref()
            .and_then(|c| c.metadata.as_ref())
            .ok_or_else(|| anyhow::anyhow!("No metadata found for token {}", token_address))?;

        let token_name = metadata.name.clone().unwrap_or_else(|| "Unknown Token".to_string());
        let token_symbol = metadata.symbol.clone().unwrap_or_else(|| "UNK".to_string());

        // For SPL tokens, we need to get additional info like decimals and supply
        // This might require additional API calls or we use defaults
        let decimals = 9; // Default for most SPL tokens
        let supply = asset.supply.map(|s| s.print_current_supply as u64);

        Ok(TokenMetadata {
            address: asset.id,
            name: token_name,
            symbol: token_symbol,
            decimals,
            supply,
            logo_uri: asset.content.as_ref()
                .and_then(|c| c.links.as_ref())
                .and_then(|l| l.image.clone()),
            creation_time: None, // Would need additional logic to determine creation time
        })
    }

    // TODO: Implement methods for:
    // - Performing security checks (requires specific Helius endpoints or logic)
}
