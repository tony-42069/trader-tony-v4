"""
Moralis API Test Script for TraderTony V4 - v2
Tests various endpoints to find working Pump Fun data sources

Usage:
    python test_moralis_api.py YOUR_MORALIS_API_KEY
"""

import sys
import json
import urllib.request
import urllib.error

def make_request(url, api_key, headers_extra=None):
    """Make a GET request to Moralis API"""
    req = urllib.request.Request(url)
    req.add_header("X-API-Key", api_key)
    req.add_header("Accept", "application/json")
    req.add_header("User-Agent", "TraderTony/1.0")

    if headers_extra:
        for k, v in headers_extra.items():
            req.add_header(k, v)

    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return json.loads(response.read().decode()), response.status
    except urllib.error.HTTPError as e:
        error_body = ""
        try:
            error_body = e.read().decode()[:300]
        except:
            pass
        return {"error": e.code, "reason": e.reason, "body": error_body}, e.code
    except Exception as e:
        return {"error": str(e)}, 0

def test_endpoint(name, url, api_key, headers_extra=None):
    """Test a single endpoint"""
    print(f"\n{'─'*60}")
    print(f"TEST: {name}")
    print(f"URL: {url}")

    data, status = make_request(url, api_key, headers_extra)

    if status == 200:
        print(f"✅ SUCCESS (HTTP {status})")
        # Pretty print first 1500 chars
        output = json.dumps(data, indent=2)
        if len(output) > 1500:
            print(output[:1500] + "\n... (truncated)")
        else:
            print(output)
        return data
    else:
        print(f"❌ FAILED (HTTP {status})")
        if isinstance(data, dict):
            if "Cloudflare" in str(data.get("body", "")):
                print("   Blocked by Cloudflare - endpoint may not exist or require different auth")
            else:
                print(f"   {data.get('reason', data.get('error', 'Unknown error'))}")
        return None

def main():
    if len(sys.argv) < 2:
        print("Usage: python test_moralis_api.py YOUR_MORALIS_API_KEY")
        print("\nGet your free API key at: https://developers.moralis.com/")
        sys.exit(1)

    api_key = sys.argv[1]

    print("="*60)
    print("MORALIS API TEST v2 - TRADER TONY V4")
    print("="*60)
    print(f"API Key: {api_key[:12]}...{api_key[-4:]} (length: {len(api_key)})")

    # Note about key format
    if api_key.startswith("eyJ"):
        print("\n⚠️  WARNING: Your key looks like a JWT token.")
        print("   Moralis API keys are usually shorter alphanumeric strings.")
        print("   Check your Moralis dashboard for the correct API key.")
        print("   Look for 'Web3 API Key' or similar in Settings > API Keys")

    print("\n" + "="*60)
    print("TESTING VARIOUS ENDPOINTS...")
    print("="*60)

    # Test 1: Basic token price (should work on any tier)
    # Using a known Solana token (USDC)
    usdc_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
    test_endpoint(
        "Token Price (Basic - verify API key works)",
        f"https://solana-gateway.moralis.io/token/mainnet/{usdc_mint}/price",
        api_key
    )

    # Test 2: Token metadata
    test_endpoint(
        "Token Metadata (Basic)",
        f"https://solana-gateway.moralis.io/token/mainnet/{usdc_mint}/metadata",
        api_key
    )

    # Test 3: Pump Fun Bonding - Original URL
    test_endpoint(
        "Pump Fun Bonding Tokens (Original URL)",
        "https://solana-gateway.moralis.io/token/mainnet/exchange/pumpfun/bonding?limit=5",
        api_key
    )

    # Test 4: Try deep-index API for discovery/filtered tokens
    test_endpoint(
        "Discovery/Filtered Tokens (deep-index)",
        "https://deep-index.moralis.io/api/v2.2/discovery/tokens?chain=solana&limit=5",
        api_key
    )

    # Test 5: Trending tokens
    test_endpoint(
        "Trending Tokens (deep-index)",
        "https://deep-index.moralis.io/api/v2.2/tokens/trending?chain=solana&limit=5",
        api_key
    )

    # Test 6: Try alternate bonding URL patterns
    test_endpoint(
        "Pump Fun Bonding (alt pattern 1)",
        "https://solana-gateway.moralis.io/token/mainnet/pumpfun/bonding?limit=5",
        api_key
    )

    # Test 7: Try with network in different position
    test_endpoint(
        "Pump Fun New Tokens",
        "https://solana-gateway.moralis.io/token/mainnet/exchange/pumpfun/new?limit=5",
        api_key
    )

    # Test 8: Check if there's a v1 or v2 API version
    test_endpoint(
        "API v2.2 Token Search",
        "https://deep-index.moralis.io/api/v2.2/tokens/search?query=pump&chain=solana&limit=5",
        api_key
    )

    print("\n" + "="*60)
    print("TEST COMPLETE")
    print("="*60)
    print("""
NEXT STEPS based on results:

1. If basic endpoints (price/metadata) FAIL:
   → Your API key may be incorrect. Check Moralis dashboard.
   → Look for "Web3 API Key" in Settings > API Keys

2. If basic endpoints WORK but Pump Fun endpoints FAIL:
   → Pump Fun endpoints may require a paid tier
   → Or the endpoint URLs have changed
   → We may need to use alternative APIs (Birdeye, Pump.fun frontend)

3. If trending/discovery endpoints WORK:
   → We can filter Solana tokens and look for Pump.fun addresses
   → Less ideal but workable

Share this output and we'll determine the best path forward!
""")

if __name__ == "__main__":
    main()
