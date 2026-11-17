"""Collect license notices from installed runtime JavaScript and locked Rust crates."""
import json
from pathlib import Path
import subprocess

sections = ["YT-Desktop — Third-party notices\n\nExternal download engines are not bundled. See README.md for their publishers and licenses.\n"]
seen = set()
license_groups = {}

def collect(name, version, directory, license_id):
    key = name, version
    if key in seen:
        return
    seen.add(key)
    notices = []
    for file in sorted(directory.iterdir()):
        if file.is_file() and file.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE")):
            notices.append("\n".join(line.rstrip() for line in file.read_text(errors="replace").splitlines()))
    text="\n\n".join(notices)
    license_groups.setdefault(text, []).append(f"{name} {version} — {license_id or 'See upstream package'}")


tree = json.loads(subprocess.check_output(["npm", "ls", "--omit=dev", "--all", "--json"]))
def npm_walk(dependencies):
    for name, data in dependencies.items():
        directory = Path("node_modules") / name
        if directory.is_dir():
            metadata = json.loads((directory / "package.json").read_text())
            collect(name, data.get("version", ""), directory, metadata.get("license", ""))
        npm_walk(data.get("dependencies", {}))
npm_walk(tree.get("dependencies", {}))
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--manifest-path", "src-tauri/Cargo.toml", "--format-version", "1", "--offline"]))
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if package["source"]:
        collect(package["name"], package["version"], Path(package["manifest_path"]).parent, package["license"])
for text, packages in license_groups.items():
    sections.append("\n" + "=" * 72 + "\n" + "\n".join(packages) + "\n\n" + text)
Path("THIRD_PARTY_NOTICES.txt").write_text("\n".join(sections).rstrip()+"\n")
print(f"Collected {len(seen)} dependency notices.")
