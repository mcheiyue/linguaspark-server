# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
# 用法：python qa/fetch_models.py <独立模型目录>；解压使用本机 Node 22.15+。
from dataclasses import dataclass
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
from urllib.request import urlopen


@dataclass(frozen=True)
class Asset:
    name: str
    location: str
    compressed_hash: str
    raw_hash: str


ASSETS = (
    Asset("model.enzh.intgemm.alphas.bin", "321191b6-2c42-4bca-84ec-b7b074b67bd6",
          "f0c96e546b0af7c1fe84647d450b1eba0b0591c167e1ead135b334e4b54f6e08",
          "4e5accc141373565ddc8fa1565bceaa8d0c3482a82cab8131c719ebcc6c2157c"),
    Asset("lex.50.50.enzh.s2t.bin", "900beaed-2dba-4a88-ad7c-852ec9e73f6a",
          "c7e4b2ee202e6cf526d07f9c5a47b759ba32b0b42a5353d3db4ee1ff3bfb5217",
          "4a5e5827788060f1d718a8132b69440929387514a045796e9b77f935db68c055"),
    Asset("srcvocab.enzh.spm", "83278466-fec4-4944-a99d-c2372fa9de88",
          "ec495ad8f0c80ba4eae5ac3b9a8e2ccb4a3a7becb1992efcfea5f4653ffdece1",
          "bd9b65504acc6d9726dd281f7defc2adb7c2c22d0688fe2f84697de25197c8c5"),
    Asset("trgvocab.enzh.spm", "84236654-1b66-42fc-9ba2-74c37f27e5eb",
          "25c6c984a7556cb6478b9f1748ff68d8fd433c0e7d472bfdbd4d64a2dffa21f1",
          "aded6993c36e440284d11cec3f6b8aef9c0e43188a772d80be342a713adf223d"),
)


def digest(path: Path) -> str:
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main() -> None:
    target = Path(sys.argv[1]).resolve() / "en-zh"
    target.mkdir(parents=True, exist_ok=True)
    node = shutil.which("node")
    if not node:
        raise SystemExit("需要已有 Node 22.15+ 解压 zstd")
    for asset in ASSETS:
        output = target / asset.name
        if output.exists() and digest(output) == asset.raw_hash:
            print(asset.name, "verified")
            continue
        compressed = output.with_suffix(output.suffix + ".zst")
        url = "https://firefox-settings-attachments.cdn.mozilla.net/main-workspace/translations-models-v2/" + asset.location + ".zst"
        if not compressed.exists():
            with urlopen(url, timeout=120) as response, compressed.open("wb") as stream:
                print("HTTP", response.status, "bytes", response.headers.get("Content-Length"), flush=True)
                shutil.copyfileobj(response, stream)
        if digest(compressed) != asset.compressed_hash:
            raise SystemExit(f"下载校验失败：{asset.name}, bytes={compressed.stat().st_size}, sha256={digest(compressed)}")
        subprocess.run([node, "-e",
                        "const fs=require('node:fs');fs.writeFileSync(process.argv[2],require('node:zlib').zstdDecompressSync(fs.readFileSync(process.argv[1])))",
                        str(compressed), str(output)], check=True)
        if digest(output) != asset.raw_hash:
            raise SystemExit(f"解压校验失败：{asset.name}")
        compressed.unlink()
        print(asset.name, asset.raw_hash, flush=True)


if __name__ == "__main__":
    main()
