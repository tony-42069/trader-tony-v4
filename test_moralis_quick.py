"""
Quick Moralis retest
"""
import sys
import json
import urllib.request
import urllib.error

def test(url, api_key):
    req = urllib.request.Request(url)
    req.add_header("X-API-Key", api_key)
    req.add_header("Accept", "application/json")
    try:
        with urllib.request.urlopen(req, timeout=30) as response:
            return json.loads(response.read().decode()), 200
    except urllib.error.HTTPError as e:
        return None, e.code

api_key = sys.argv[1]

print("Testing Moralis Pump Fun endpoints...\n")

# Test bonding
print("1. Bonding tokens: ", end="")
data, status = test("https://solana-gateway.moralis.io/token/mainnet/exchange/pumpfun/bonding?limit=3", api_key)
if status == 200:
    tokens = data.get("result", [])
    print(f"✅ SUCCESS - Got {len(tokens)} tokens")
    if tokens:
        t = tokens[0]
        print(f"   First: {t.get('symbol')} | Progress: {t.get('bondingCurveProgress', 'N/A')}% | MCap: ${t.get('fullyDilutedValuation', 'N/A')}")
else:
    print(f"❌ FAILED (HTTP {status})")

# Test graduated
print("\n2. Graduated tokens: ", end="")
data, status = test("https://solana-gateway.moralis.io/token/mainnet/exchange/pumpfun/graduated?limit=3", api_key)
if status == 200:
    tokens = data.get("result", [])
    print(f"✅ SUCCESS - Got {len(tokens)} tokens")
    if tokens:
        print(f"   First: {json.dumps(tokens[0], indent=2)[:500]}")
else:
    print(f"❌ FAILED (HTTP {status})")

# Test new
print("\n3. New tokens: ", end="")
data, status = test("https://solana-gateway.moralis.io/token/mainnet/exchange/pumpfun/new?limit=3", api_key)
if status == 200:
    tokens = data.get("result", [])
    print(f"✅ SUCCESS - Got {len(tokens)} tokens")
else:
    print(f"❌ FAILED (HTTP {status})")

# Test basic endpoint (should always work)
print("\n4. Basic price endpoint: ", end="")
data, status = test("https://solana-gateway.moralis.io/token/mainnet/EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v/price", api_key)
if status == 200:
    print(f"✅ SUCCESS - USDC price: ${data.get('usdPrice', 'N/A')}")
else:
    print(f"❌ FAILED (HTTP {status})")

print("\n" + "="*50)
print("If bonding works but graduated doesn't, we can still")
print("use Moralis for Final Stretch strategy!")
