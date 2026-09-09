#!/usr/bin/env python3
"""Verify Cargo's packaged artifacts and bind later uploads to those artifacts."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile


def run(args, source, *, stream=False):
    """Keep Cargo diagnostics available without exposing any environment values."""
    if not stream:
        return subprocess.check_output(args, cwd=source, text=True).strip()
    lines = []
    with subprocess.Popen(
        args,
        cwd=source,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        env={**os.environ, "CARGO_TERM_COLOR": "never"},
    ) as process:
        for line in process.stdout:
            print(line, end="", flush=True)
            lines.append(line)
        if process.wait():
            raise subprocess.CalledProcessError(process.returncode, args)
    return "".join(lines)


def digest(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def identity(source):
    root = Path(run(["git", "rev-parse", "--show-toplevel"], source)).resolve()
    if root != source:
        raise ValueError("source-dir must be the release checkout root")
    if run(["git", "status", "--porcelain=v1", "--untracked-files=all"], source):
        raise ValueError("release source checkout must be clean")
    return {
        "source_sha": run(["git", "rev-parse", "HEAD"], source),
        "lock_sha256": digest(source / "Cargo.lock"),
        "cargo": run(["cargo", "--version"], source),
        "rustc": run(["rustc", "--version"], source),
    }


def metadata(source):
    return json.loads(
        run(
            [
                "cargo",
                "metadata",
                "--no-deps",
                "--locked",
                "--format-version",
                "1",
            ],
            source,
        )
    )


def publishable(data):
    members = set(data["workspace_members"])
    return {
        package["name"]: package["version"]
        for package in data["packages"]
        if package["id"] in members
        and (package["publish"] is None or "crates-io" in package["publish"])
    }


def cargo_options():
    return ["--locked", "--all-features", "--registry", "crates-io"]


def verify_archive(path, name, version, source_sha):
    with tarfile.open(path) as archive:
        member = archive.extractfile(f"{name}-{version}/.cargo_vcs_info.json")
        if member is None:
            raise ValueError(f"{name}: packaged VCS information is missing")
        info = json.load(member)["git"]
    if info.get("sha1") != source_sha or info.get("dirty", False):
        raise ValueError(f"{name}: archive does not identify the verified clean source")


def prepare(args):
    source = args.source_dir.resolve()
    output = args.proof_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "verification.json"
    # A failed attempt must never leave an earlier success marker behind.
    report_path.unlink(missing_ok=True)
    before = identity(source)
    data = metadata(source)
    packages = publishable(data)
    version = args.tag.removeprefix("v")
    if (
        not args.tag.startswith("v")
        or not packages
        or set(packages.values()) != {version}
    ):
        raise ValueError("release tag must match every publishable workspace package")
    selection = ["--workspace"]
    for package in data["packages"]:
        if (
            package["id"] in data["workspace_members"]
            and package["name"] not in packages
        ):
            selection.extend(["--exclude", package["name"]])
    log = run(
        [
            "cargo",
            "package",
            *selection,
            *cargo_options(),
        ],
        source,
        stream=True,
    )
    # Cargo packages dependencies first to populate its temporary registry.
    order = re.findall(r"^\s*Packaging ([a-zA-Z0-9_-]+) v[^\s]+", log, re.MULTILINE)
    if len(order) != len(packages) or set(order) != set(packages):
        raise ValueError(
            "Cargo packaging did not include every publishable crate exactly once"
        )
    if identity(source) != before:
        raise ValueError("release inputs changed during package verification")
    records = []
    for name in order:
        filename = f"{name}-{version}.crate"
        archive = Path(data["target_directory"]) / "package" / filename
        verify_archive(archive, name, version, before["source_sha"])
        shutil.copyfile(archive, output / filename)
        records.append({
            "crate": name,
            "version": version,
            "archive": filename,
            "sha256": digest(archive),
        })
    report = {
        "schema": 1,
        "tag": args.tag,
        "helper_sha": args.helper_sha,
        **before,
        "crates": records,
    }
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    matrix = [{"crate": name} for name in order]
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output_file:
            output_file.write(f"crates={json.dumps(matrix, separators=(',', ':'))}\n")
    summary = [
        "## Verified crate packages",
        "",
        f"Release: `{args.tag}`",
        f"Source commit: `{before['source_sha']}`",
        f"Helper commit: `{args.helper_sha}`",
        f"Toolchain: `{before['cargo']}` / `{before['rustc']}`",
        "",
        "Every listed archive passed Cargo's extracted-package build with all features.",
        "No registry upload occurred during this verification.",
        "",
        "| Crate | Version | Archive SHA-256 |",
        "| --- | --- | --- |",
        *[f"| {r['crate']} | {version} | `{r['sha256']}` |" for r in records],
        "",
    ]
    (output / "verification.md").write_text("\n".join(summary), encoding="utf-8")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(
            os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8"
        ) as summary_file:
            summary_file.write("\n".join(summary))
    print(f"Verified {len(records)} packaged crates; report: {report_path}")


def check(args, *, repackage):
    source = args.source_dir.resolve()
    proof = args.proof_dir.resolve()
    report = json.loads((proof / "verification.json").read_text(encoding="utf-8"))
    if (
        report["schema"] != 1
        or report["tag"] != args.tag
        or report["helper_sha"] != args.helper_sha
    ):
        raise ValueError(
            "verification report does not match this release/helper revision"
        )
    for key, value in identity(source).items():
        if report[key] != value:
            raise ValueError(f"release input differs from verification: {key}")
    records = [record for record in report["crates"] if record["crate"] == args.crate]
    if len(records) != 1 or records[0]["version"] != args.tag.removeprefix("v"):
        raise ValueError("crate/version is absent from verified package set")
    record = records[0]
    filename = f"{args.crate}-{record['version']}.crate"
    if record["archive"] != filename or Path(filename).name != filename:
        raise ValueError("invalid verified archive filename")
    if digest(proof / filename) != record["sha256"]:
        raise ValueError("verified archive checksum mismatch")
    if repackage:
        run(
            [
                "cargo",
                "package",
                "-p",
                args.crate,
                "--no-verify",
                *cargo_options(),
            ],
            source,
            stream=True,
        )
        archive = Path(metadata(source)["target_directory"]) / "package" / filename
        if digest(archive) != record["sha256"]:
            raise ValueError(
                f"{args.crate}: upload package differs from the verified archive"
            )
        print(f"{args.crate}: upload package matches verified archive")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "command",
        nargs="?",
        default="prepare",
        choices=["prepare", "inputs", "package"],
    )
    parser.add_argument(
        "--source-dir",
        type=Path,
        default=os.environ.get("SOURCE_DIR"),
        required="SOURCE_DIR" not in os.environ,
    )
    parser.add_argument(
        "--proof-dir",
        type=Path,
        default=os.environ.get("PROOF_DIR"),
        required="PROOF_DIR" not in os.environ,
    )
    parser.add_argument(
        "--tag",
        default=os.environ.get("RELEASE_TAG"),
        required="RELEASE_TAG" not in os.environ,
    )
    parser.add_argument(
        "--helper-sha",
        default=os.environ.get("HELPER_SHA"),
        required="HELPER_SHA" not in os.environ,
    )
    parser.add_argument("--crate")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.helper_sha):
        parser.error("helper-sha must be a full commit SHA")
    if args.command != "prepare" and not args.crate:
        parser.error("--crate is required for publication checks")
    try:
        if args.command == "prepare":
            prepare(args)
        else:
            check(args, repackage=args.command == "package")
    except (
        OSError,
        ValueError,
        KeyError,
        tarfile.TarError,
        subprocess.CalledProcessError,
    ) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
