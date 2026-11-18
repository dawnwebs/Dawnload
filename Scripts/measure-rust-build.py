#!/usr/bin/env python3
"""Build release binaries and record wall time and uncompressed sizes on each CI runner."""
import json
import os
import pathlib
import platform
import subprocess
import time

started = time.perf_counter()
subprocess.run(
    ["cargo", "build", "--release", "--locked", "-p", "dawnload-cli", "-p", "dawnload-ui"],
    check=True,
)
suffix = ".exe" if platform.system() == "Windows" else ""
sizes = {
    name: pathlib.Path(f"target/release/{name}{suffix}").stat().st_size
    for name in ["dawnload", "dawnload-app"]
}
measurement = {
    "os": platform.platform(),
    "architecture": platform.machine(),
    "release_build_seconds": round(time.perf_counter() - started, 2),
    "binary_bytes": sizes,
    "note": "Release step wall time; preceding debug checks and restored caches affect it.",
}
pathlib.Path("target/release-measurements.json").write_text(json.dumps(measurement, indent=2) + "\n")
print(json.dumps(measurement, indent=2))
if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
    with open(summary, "a") as report:
        report.write("### Release build measurement\n\n```json\n" + json.dumps(measurement, indent=2) + "\n```\n")
