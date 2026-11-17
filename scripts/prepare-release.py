"""Sign downloaded CI artifacts locally and create a Tauri update manifest.

Private keys are never copied into the output directory or sent to CI.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
from urllib.parse import quote

parser = argparse.ArgumentParser()
parser.add_argument("input", type=Path, help="Directory with extracted CI artifacts")
parser.add_argument("output", type=Path)
parser.add_argument("--key", type=Path, required=True, help="Local Tauri private key path")
parser.add_argument("--version", required=True)
args = parser.parse_args()
if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", args.version):
    raise SystemExit("Use a semantic version without v")
args.output.mkdir(parents=True, exist_ok=True)
platforms = {}
for platform, extension in [("darwin-aarch64", "app.tar.gz"), ("darwin-x86_64", "app.tar.gz"), ("windows-x86_64", "setup.exe"), ("linux-x86_64", "AppImage")]:
    name = f"YT-Desktop_{args.version}_{platform}.{extension}"
    matches = list(args.input.rglob(name))
    if len(matches) != 1:
        raise SystemExit(f"Expected one {name}; found {len(matches)}")
    target = args.output / name
    shutil.copy2(matches[0], target)
    result = subprocess.run([str(Path("node_modules/.bin/tauri").resolve()), "signer", "sign", "--private-key-path", str(args.key.resolve()), "--password", "", "--app-version", args.version, str(target.resolve())], capture_output=True, text=True)
    if result.returncode:
        raise SystemExit(f"Signing failed for {name}; no private key output was printed")
    signature = Path(str(target) + ".sig").read_text().strip()
    platforms[platform] = {"signature": signature, "url": f"https://github.com/it8090/yt-desktop/releases/download/v{args.version}/{quote(name)}"}
for package in args.input.rglob(f"YT-Desktop_{args.version}_*.deb"):
    shutil.copy2(package, args.output / package.name)
manifest = {"version": args.version, "notes": Path("docs/发布说明.md").read_text(), "pub_date": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"), "platforms": platforms}
(args.output / "latest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
checksums = [f"{hashlib.file_digest(file.open('rb'), 'sha256').hexdigest()}  {file.name}" for file in sorted(args.output.iterdir()) if file.is_file() and file.name != "SHA256SUMS"]
(args.output / "SHA256SUMS").write_text("\n".join(checksums) + "\n")
print("Signed packages, version-bound signatures, latest.json and SHA256SUMS are ready.")
