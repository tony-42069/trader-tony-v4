"""
Test Solana holder stats endpoint - CORRECT URL
"""
import sys
import json
import urllib.request
import urllib.error

if len(sys.argv) < 2:
    print("Usage: python test_holders.py YOUR_API_KEY")
    sys.exit(1)

api_key = sys.argv[1]

def make_request(url, api_key):
    req = urllib.request.Request(url)
    req.add_header("X-API-Key", api_key)
    req.add_header("Accept", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return json.loads(response.read().decode()), 200
    except urllib.error.HTTPError as e:
        return None, e.code

# Step 1: Get a bonding token
print("Step 1: Getting a bonding token...")
url = "https://solana-gateway.moralis.io/token/mainnet/exchange/pumpfun/bonding?limit=1"
data, status = make_request(url, api_key)

if status != 200:
    print(f"Failed to get bonding tokens (HTTP {status})")
    sys.exit(1)

token = data["result"][0]
token_address = token["tokenAddress"]
print(f"✅ Got token: {token['symbol']} ({token_address})")
print(f"   Progress: {token['bondingCurveProgress']}%")
print(f"   Market Cap: ${token['fullyDilutedValuation']}")

# Step 2: Test holder STATS endpoint (correct URL for total count)
print(f"\nStep 2: Testing holder stats endpoint...")
url = f"https://solana-gateway.moralis.io/token/mainnet/holders/{token_address}"
print(f"URL: {url}")

data, status = make_request(url, api_key)

if status == 200:
    print("✅ SUCCESS!")
    print(json.dumps(data, indent=2))

    if "totalHolders" in data:
        print(f"\n🎯 TOTAL HOLDERS: {data['totalHolders']}")
else:
    print(f"❌ FAILED (HTTP {status})")

print("\n" + "="*50)
print("If this works, we have EVERYTHING for Final Stretch!")
print("- bondingCurveProgress from /exchange/pumpfun/bonding")
print("- fullyDilutedValuation (market cap) from same")
print("- totalHolders from /token/mainnet/holders/{address}")
