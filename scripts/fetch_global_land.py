"""Fetch the pinned, rights-reviewed Natural Earth land source archive.

This downloads bytes only. Geographic processing and its independent checks
are in prepare_global_land.py and the water-mask verification tools.
"""
from hashlib import sha256
from pathlib import Path
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[1]
URL = "https://naciscdn.org/naturalearth/10m/physical/ne_10m_land.zip"
SHA = "e547d749445eaa0964aba76738090ec88f5e63c4585122170f98c67a7ea922dc"
MAX_BYTES = 10_000_000


def main():
    target = ROOT / ".cache/natural-earth/ne_10m_land.zip"
    if target.is_file() and sha256(target.read_bytes()).hexdigest() == SHA:
        print("Verified cached Natural Earth land v5.1.1")
        return
    # Source rights reviewed against the provider's actual public-domain terms:
    # https://www.naturalearthdata.com/about/terms-of-use/ (2026-10-22).
    with urlopen(URL, timeout=60) as response:
        data = response.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES or sha256(data).hexdigest() != SHA:
        raise ValueError("Source bytes changed; new source review required")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
    print("Verified Natural Earth land v5.1.1; public-domain terms in NOTICE.md")


if __name__ == "__main__":
    main()
