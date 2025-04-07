# TraderTony V4 API Usage

This document outlines the primary external APIs used by TraderTony V4.

## Helius API

Helius provides various RPC and enhanced APIs for Solana. TraderTony primarily uses the Digital Asset Standard (DAS) API for querying asset information.

### DAS API - `searchAssets`

Used for discovering tokens, potentially filtering by criteria like creation time or specific interfaces (though current implementation is basic).

-   **Endpoint:** `https://api.helius.xyz/v0/das/searchAssets` (Note: Uses v0 for DAS)
-   **Method:** `POST`
-   **Authentication:** Requires Helius API key passed via `api-key` query parameter (`?api-key=YOUR_API_KEY`).
-   **Request Body Example:**
    ```json
    {
      "limit": 100,
      "sortBy": { "sortBy": "created", "sortDirection": "desc" },
      "burnt": false
      // Other filters like ownerAddress, interface, etc. can be added
    }
    ```
-   **Response Structure (Simplified):** The actual response is nested under a `result` field.
    ```json
    {
      "jsonrpc": "2.0",
      "result": {
        "total": 12345,
        "limit": 100,
        "page": 1,
        "items": [
          {
            "id": "TokenMintAddress", // e.g., SPL Token Mint
            "interface": "FungibleToken", // Or V1_NFT, etc.
            "content": {
              "metadata": {
                "name": "Token Name",
                "symbol": "SYMBOL"
              },
              "links": {
                  "image": "..." // Optional image URL
              }
            },
            "ownership": {
                "owner": "...",
                "frozen": false
            },
            "supply": {
                "print_current_supply": 1000000000 // Example supply
            },
            "authorities": [
                { "address": "...", "scopes": ["mint"] } // Example mint authority
            ]
            // ... other DAS fields
          }
        ]
      },
      "id": "rpc-request-id"
    }
    ```
-   **Usage in TraderTony:** `src/api/helius.rs` uses this to find potential new tokens (currently basic implementation).

*(Refer to the official [Helius DAS API documentation](https://docs.helius.xyz/digital-asset-standard-das-api/search-assets) for detailed request/response schemas and filtering options).*

## Jupiter API (v6)

Jupiter provides APIs for finding the best swap routes and executing trades across Solana DEXs.

### Get Quote (`/quote`)

Used to find the best price and route for a potential swap.

-   **Endpoint:** `https://quote-api.jup.ag/v6/quote`
-   **Method:** `GET`
-   **Query Parameters:**
    -   `inputMint`: Mint address of the token you are selling.
    -   `outputMint`: Mint address of the token you want to buy.
    -   `amount`: The amount of `inputMint` token to sell (in the smallest unit, e.g., lamports).
    -   `slippageBps`: Slippage tolerance in basis points (e.g., `50` for 0.5%).
    -   `onlyDirectRoutes` (optional, `true`/`false`): Restrict to direct routes only.
    -   `asLegacyTransaction` (optional, `true`/`false`): Request a legacy transaction format (default is `false` for VersionedTransaction).
    -   *Other parameters available, see Jupiter docs.*
-   **Response Structure (Key Fields):**
    ```json
    {
      "inputMint": "...",
      "inAmount": "1000000000", // Amount of input token (lamports)
      "outputMint": "...",
      "outAmount": "987654321", // Estimated amount of output token (lamports)
      "otherAmountThreshold": "982716049", // Minimum outAmount considering slippage
      "swapMode": "ExactIn",
      "slippageBps": 50,
      "priceImpactPct": "0.0123", // Price impact percentage as a string
      "routePlan": [ /* Details of the swap route(s) */ ],
      // ... other fields like platformFee, contextSlot, timeTaken
    }
    ```
-   **Usage in TraderTony:** `src/api/jupiter.rs` uses this before executing a swap.

### Get Swap Transaction (`/swap`)

Used to get the serialized transaction needed to execute the swap based on a prior quote.

-   **Endpoint:** `https://quote-api.jup.ag/v6/swap`
-   **Method:** `POST`
-   **Request Body:**
    ```json
    {
      "quoteResponse": { /* The full JSON response from the /quote endpoint */ },
      "userPublicKey": "YourWalletPublicKey", // The public key of the wallet signing the transaction
      "wrapAndUnwrapSol": true, // Automatically handle SOL wrapping/unwrapping
      "dynamicComputeUnitLimit": true, // Recommended: Let Jupiter estimate CU limit
      "computeUnitPriceMicroLamports": 50000 // Optional: Set priority fee
    }
    ```
-   **Response Structure:**
    ```json
    {
      "swapTransaction": "BASE64_ENCODED_VERSIONED_TRANSACTION", // The transaction to sign and send
      "lastValidBlockHeight": 123456789, // Block height for transaction validity
      "prioritizationFeeLamports": 10000 // Optional: Fee used in the transaction
    }
    ```
-   **Usage in TraderTony:** `src/api/jupiter.rs` calls this after getting a quote, then decodes, signs (via `WalletManager`), and sends the `swapTransaction`.

*(Refer to the official [Jupiter API documentation](https://docs.jup.ag/jupiter-api) for complete details).*

## Telegram Bot API

TraderTony uses the `teloxide` crate, which acts as a wrapper around the standard Telegram Bot API.

-   **Interaction:** Handled via commands (`/start`, `/help`, etc.) and inline keyboard callbacks.
-   **Implementation:** See `src/bot/commands.rs`, `src/bot/keyboards.rs`, and the `main.rs` dispatcher setup.
-   **Authentication:** Uses the `TELEGRAM_BOT_TOKEN` from the `.env` file.
-   **Authorization:** Checks the incoming message's user ID against `TELEGRAM_ADMIN_USER_ID` from `.env`.
