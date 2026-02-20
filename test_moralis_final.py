"""
Moralis API Final Test - Fixed Version
"""

import sys
import json
import urllib.request
import urllib.error

def make_request(url, api_key):
    req = urllib.request.Request(url)
    req.add_header("X-API-Key", api_key)
    req.add_header("Accept", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return json.loads(response.read().decode()), 200
    except urllib.error.HTTPError as e:
        body = ""
        try:
            body = e.read().decode()[:200]
        except:
            pass
        return {"error": e.code, "body": body}, e.code
    except Exception as e:
        return {"error": str(e)}, 0

def main():
    if len(sys.argv) < 2:
        print("Usage: python test_moralis_final.py YOUR_API_KEY")
        sys.exit(1)

    api_key = sys.argv[1]
    test_token = None

    print("="*60)
    print("MORALIS FINAL TEST - Fixed Version")
    print("="*60)

    # Test 1: Bonding tokens (confirm it still works, get a token address)
    print("\n" + "─"*60)
    print("TEST 1: Bonding Tokens (confirm working)")
    print("─"*60)
    url = "https://solana-gateway.moralis.io/token/mainnet/exchange/pumpfun/bonding?limit=3"
    data, status = make_request(url, api_key)

    if status == 200:
        print("✅ SUCCESS!")
        tokens = data.get("result", [])
        if tokens and len(tokens) > 0:
            test_token = tokens[0].get("tokenAddress")
            print(f"Got {len(tokens)} bonding tokens")
            print(f"First token: {test_token}")
            print(f"Progress: {tokens[0].get('bondingCurveProgress')}%")
            print(f"Market Cap: ${tokens[0].get('fullyDilutedValuation')}")
        else:
            print("No tokens in result")
    else:
        print(f"❌ FAILED: {data}")

    # Test 2: Graduated tokens
    print("\n" + "─"*60)
    print("TEST 2: Graduated Tokens")
    print("─"*60)
    url = "https://solana-gateway.moralis.io/token/mainnet/exchange/pumpfun/graduated?limit=3"
    data, status = make_request(url, api_key)

    if status == 200:
        print("✅ SUCCESS!")
        print(json.dumps(data, indent=2)[:1000])
    else:
        print(f"❌ FAILED (HTTP {status})")
        if status == 403:
            print("   → This endpoint may require a paid tier")
            print("   → We can work around this using trending tokens + on-chain checks")

    # Test 3: Holder stats (if we have a token)
    print("\n" + "─"*60)
    print("TEST 3: Holder Stats")
    print("─"*60)

    if test_token:
        url = f"https://solana-gateway.moralis.io/token/mainnet/holders/{test_token}"
        print(f"URL: {url}")
        data, status = make_request(url, api_key)

        if status == 200:
            print("✅ SUCCESS!")
            print(json.dumps(data, indent=2)[:1500])
        else:
            print(f"❌ FAILED (HTTP {status}): {data}")
    else:
        print("⚠️ Skipped - no token from previous test")

    # Test 4: Bonding status for specific token
    print("\n" + "─"*60)
    print("TEST 4: Single Token Bonding Status")
    print("─"*60)

    if test_token:
        url = f"https://solana-gateway.moralis.io/token/mainnet/{test_token}/bonding-status"
        print(f"URL: {url}")
        data, status = make_request(url, api_key)

        if status == 200:
            print("✅ SUCCESS!")
            print(json.dumps(data, indent=2))
        else:
            print(f"❌ FAILED (HTTP {status}): {data}")
    else:
        print("⚠️ Skipped - no token from previous test")

    # Test 5: Token metadata (to check for graduation indicators)
    print("\n" + "─"*60)
    print("TEST 5: Token Pairs (to see if graduated to Raydium)")
    print("─"*60)

    if test_token:
        url = f"https://solana-gateway.moralis.io/token/mainnet/{test_token}/pairs"
        print(f"URL: {url}")
        data, status = make_request(url, api_key)

        if status == 200:
            print("✅ SUCCESS!")
            print(json.dumps(data, indent=2)[:1500])
        else:
            print(f"❌ FAILED (HTTP {status})")

    print("\n" + "="*60)
    print("SUMMARY")
    print("="*60)
    print("""
WORKING ENDPOINTS:
✅ /exchange/pumpfun/bonding - Lists tokens in bonding phase with progress %
✅ /exchange/pumpfun/new - Lists newly created tokens

POTENTIALLY PREMIUM:
❓ /exchange/pumpfun/graduated - May require paid tier

FOR FINAL STRETCH STRATEGY:
→ Use /exchange/pumpfun/bonding (works!)
→ Filter: bondingCurveProgress >= 20, fullyDilutedValuation >= 20000
→ Then call /holders/{address} for holder count

FOR MIGRATED STRATEGY (workaround if graduated is premium):
→ Use /bonding-status to check if complete=true
→ Or use trending tokens and filter for graduated ones
→ Or check on-chain if bonding curve is complete
""")

if __name__ == "__main__":
    main()
