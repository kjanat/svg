#!/usr/bin/env python3
"""Record the two checkout revisions and the release source's Rust toolchain."""

import argparse
import os
from pathlib import Path
import re
import subprocess
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-dir", type=Path, default=Path("source"))
    parser.add_argument("--helper-dir", type=Path, default=Path("."))
    parser.add_argument(
        "--expected-source", default=os.environ.get("EXPECTED_SOURCE_SHA")
    )
    parser.add_argument(
        "--expected-helper", default=os.environ.get("EXPECTED_HELPER_SHA")
    )
    args = parser.parse_args()
    values = {}
    for kind, directory, expected in (
        ("source", args.source_dir, args.expected_source),
        ("helper", args.helper_dir, args.expected_helper),
    ):
        sha = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=directory, text=True
        ).strip()
        if expected and sha != expected:
            parser.error(f"{kind} checkout changed since release verification")
        values[f"{kind}_sha"] = sha
    with (args.source_dir / "rust-toolchain.toml").open("rb") as file:
        toolchain = tomllib.load(file)["toolchain"]["channel"]
    if not re.fullmatch(r"[A-Za-z0-9._-]+", toolchain):
        parser.error("invalid release toolchain channel")
    values["toolchain"] = toolchain
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        for name, value in values.items():
            output.write(f"{name}={value}\n")
            print(f"{name}: {value}")


if __name__ == "__main__":
    main()
