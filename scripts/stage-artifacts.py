"""Normalize native build outputs for GitHub Actions artifacts."""
from pathlib import Path
import json
import shutil
import sys
import tarfile

system, arch = sys.argv[1:3]
arch = {"ARM64": "aarch64", "X64": "x86_64"}[arch]
version = json.loads(Path("package.json").read_text())["version"]
root = Path("src-tauri/target/release/bundle")
out = Path("artifacts")
out.mkdir(exist_ok=True)
if system == "macOS":
    target = out / f"YT-Desktop_{version}_darwin-{arch}.app.tar.gz"
    with tarfile.open(target, "w:gz") as archive:
        archive.add(root / "macos/YT-Desktop.app", arcname="YT-Desktop.app")
else:
    patterns = [("nsis/*.exe", "windows", "setup.exe")] if system == "Windows" else [("appimage/*.AppImage", "linux", "AppImage"), ("deb/*.deb", "linux", "deb")]
    for pattern, platform, extension in patterns:
        matches = list(root.glob(pattern))
        if len(matches) != 1:
            raise SystemExit(f"Expected one {pattern}, got {len(matches)}")
        shutil.copy2(matches[0], out / f"YT-Desktop_{version}_{platform}-{arch}.{extension}")
