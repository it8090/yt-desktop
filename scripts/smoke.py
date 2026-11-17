"""Create a local synthetic media fixture and run the real download integration test."""
import functools
import http.server
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading

with tempfile.TemporaryDirectory(prefix="yt-desktop-smoke-") as directory:
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        raise SystemExit("ffmpeg is required")
    subprocess.run([ffmpeg, "-y", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", "sine=frequency=440", "-t", "5", "-c:v", "libx264", "-c:a", "aac", "-movflags", "+faststart", str(Path(directory) / "fixture.mp4")], check=True)
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=directory)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    environment = {**os.environ, "YT_SMOKE_URL": f"http://127.0.0.1:{server.server_port}/fixture.mp4"}
    try:
        subprocess.run(["cargo", "test", "--manifest-path", "src-tauri/Cargo.toml", "--test", "download_flow", "--", "--ignored", "--nocapture"], env=environment, check=True)
    finally:
        server.shutdown()
