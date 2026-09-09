"""Real Cargo regressions for release preparation and package identity checks."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
VERIFY = ROOT / ".github/actions/crates-verify/verify.py"
IDENTITIES = VERIFY.with_name("identities.py")
HELPER_SHA = "a" * 40
VERSION = "0.0.0-unpublished.4344"
TAG = f"v{VERSION}"


def execute(args, cwd, **kwargs):
    result = subprocess.run(args, cwd=cwd, text=True, capture_output=True, **kwargs)
    if result.returncode:
        raise AssertionError(result.stdout + result.stderr)
    return result


def fixture(path, *, broken=False):
    source = path / "source"
    source.mkdir()
    (source / ".gitignore").write_text("/target\n")
    (source / "rust-toolchain.toml").write_bytes(
        (ROOT / "rust-toolchain.toml").read_bytes()
    )
    (source / "Cargo.toml").write_text(
        '[workspace]\nmembers = ["leaf", "app", "private"]\nresolver = "3"\n'
    )
    for name in ("leaf", "app", "private"):
        directory = source / name
        (directory / "src").mkdir(parents=True)
        manifest = f'[package]\nname = "svg-release-fixture-{name}"\nversion = "{VERSION}"\nedition = "2024"\n'
        manifest += 'description = "Release test fixture"\nlicense = "MIT"\n'
        if name == "private":
            manifest += "publish = false\n"
        if broken and name == "leaf":
            manifest += 'exclude = ["required.txt"]\n'
        if name == "app":
            manifest += f'[dependencies]\nsvg-release-fixture-leaf = {{ path = "../leaf", version = "={VERSION}" }}\n'
            code = (
                "pub fn value() -> &'static str { svg_release_fixture_leaf::value() }\n"
            )
        else:
            code = 'pub fn value() -> &\'static str { "fixture" }\n'
        if broken and name == "leaf":
            (directory / "required.txt").write_text("needed by the build")
            code = (
                'pub fn value() -> &\'static str { include_str!("../required.txt") }\n'
            )
        (directory / "Cargo.toml").write_text(manifest)
        (directory / "src/lib.rs").write_text(code)
    execute(["git", "init", "--quiet"], source)
    execute(["cargo", "generate-lockfile", "--offline"], source)
    execute(["git", "add", "."], source)
    execute(
        [
            "git",
            "-c",
            "user.name=Release tests",
            "-c",
            "user.email=release-tests@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
        source,
    )
    return source


def verify(
    source, proof, command="prepare", *, crate=None, helper=HELPER_SHA, extra_env=None
):
    args = [
        sys.executable,
        str(VERIFY),
        command,
        "--source-dir",
        str(source),
        "--proof-dir",
        str(proof),
        "--tag",
        TAG,
        "--helper-sha",
        helper,
    ]
    if crate:
        args.extend(["--crate", crate])
    env = {
        key: value
        for key, value in os.environ.items()
        if key not in {"GITHUB_OUTPUT", "GITHUB_STEP_SUMMARY"}
    }
    # Release workflows request colored diagnostics; order parsing must handle it.
    env["CARGO_TERM_COLOR"] = "always"
    env.update(extra_env or {})
    return subprocess.run(args, cwd=source, env=env, text=True, capture_output=True)


class PackageVerificationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix="svg-crate-verification-")
        cls.addClassCleanup(cls.temp.cleanup)
        cls.directory = Path(cls.temp.name)
        cls.source = fixture(cls.directory)
        cls.proof = cls.directory / "proof"
        cls.prepared = verify(cls.source, cls.proof)
        if cls.prepared.returncode:
            raise AssertionError(cls.prepared.stdout + cls.prepared.stderr)
        cls.report_path = cls.proof / "verification.json"
        cls.original_report = cls.report_path.read_bytes()

    def tearDown(self):
        self.report_path.write_bytes(self.original_report)

    def test_unpublished_siblings_are_built_from_packaged_registry_sources(self):
        report = json.loads(self.original_report)
        self.assertEqual(
            [p["crate"] for p in report["crates"]],
            ["svg-release-fixture-leaf", "svg-release-fixture-app"],
        )
        self.assertIn("Verifying svg-release-fixture-leaf", self.prepared.stdout)
        self.assertIn("Verifying svg-release-fixture-app", self.prepared.stdout)
        # The app's lockfile must bind the dependency to the actual staged crate.
        import tarfile

        with tarfile.open(self.proof / report["crates"][1]["archive"]) as archive:
            lock = (
                archive
                .extractfile(f"svg-release-fixture-app-{VERSION}/Cargo.lock")
                .read()
                .decode()
            )
        self.assertIn(report["crates"][0]["sha256"], lock)
        self.assertIn("registry+https://github.com/rust-lang/crates.io-index", lock)
        self.assertNotIn("Uploading ", self.prepared.stdout)
        self.assertEqual(
            report["source_sha"],
            execute(["git", "rev-parse", "HEAD"], self.source).stdout.strip(),
        )

    def test_repackaged_leaf_matches_the_verified_archive(self):
        result = verify(
            self.source, self.proof, "package", crate="svg-release-fixture-leaf"
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("upload package matches verified archive", result.stdout)

    def test_dependent_package_matches_after_sibling_registry_handoff(self):
        # Populate a Cargo local registry from the verified .crate files. This
        # models siblings appearing in the registry without an external upload.
        registry = self.directory / "registry"
        registry.mkdir()
        report = json.loads(self.original_report)
        for record in report["crates"]:
            name = record["crate"]
            (registry / record["archive"]).write_bytes(
                (self.proof / record["archive"]).read_bytes()
            )
            index = registry / "index" / name[:2] / name[2:4]
            index.mkdir(parents=True, exist_ok=True)
            deps = []
            if name.endswith("-app"):
                deps.append({
                    "name": "svg-release-fixture-leaf",
                    "req": f"={VERSION}",
                    "features": [],
                    "optional": False,
                    "default_features": True,
                    "target": None,
                    "kind": "normal",
                })
            (index / name).write_text(
                json.dumps({
                    "name": name,
                    "vers": VERSION,
                    "deps": deps,
                    "cksum": record["sha256"],
                    "features": {},
                    "yanked": False,
                })
                + "\n"
            )
        cargo_home = self.directory / "registry-client"
        cargo_home.mkdir()
        (cargo_home / "config.toml").write_text(
            '[source.crates-io]\nreplace-with = "fixture"\n'
            f"[source.fixture]\nlocal-registry = {json.dumps(registry.as_posix())}\n"
            "[net]\noffline = true\n"
        )
        result = verify(
            self.source,
            self.proof,
            "package",
            crate="svg-release-fixture-app",
            extra_env={"CARGO_HOME": str(cargo_home)},
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("upload package matches verified archive", result.stdout)

    def test_repackaging_rejects_a_different_archive_even_with_a_matching_report_hash(
        self,
    ):
        report = json.loads(self.original_report)
        record = report["crates"][0]
        archive = self.proof / record["archive"]
        original = archive.read_bytes()
        try:
            archive.write_bytes(original + b"different package")
            record["sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
            self.report_path.write_text(json.dumps(report))
            result = verify(self.source, self.proof, "package", crate=record["crate"])
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(
                "upload package differs from the verified archive", result.stderr
            )
        finally:
            archive.write_bytes(original)

    def test_report_must_match_source_toolchain_and_helper(self):
        for field in (
            "source_sha",
            "lock_sha256",
            "cargo",
            "rustc",
            "tag",
            "helper_sha",
        ):
            with self.subTest(field=field):
                report = json.loads(self.original_report)
                report[field] = "different"
                self.report_path.write_text(json.dumps(report))
                result = verify(
                    self.source, self.proof, "inputs", crate="svg-release-fixture-leaf"
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("error:", result.stderr)

    def test_dirty_source_cannot_reuse_verification(self):
        path = self.source / "leaf/src/lib.rs"
        original = path.read_bytes()
        try:
            path.write_bytes(original + b"\n// changed after verification\n")
            result = verify(
                self.source, self.proof, "inputs", crate="svg-release-fixture-leaf"
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("must be clean", result.stderr)
        finally:
            path.write_bytes(original)

    def test_changed_archive_and_unverified_crate_are_rejected(self):
        record = json.loads(self.original_report)["crates"][0]
        archive = self.proof / record["archive"]
        original = archive.read_bytes()
        try:
            archive.write_bytes(original + b"changed")
            result = verify(self.source, self.proof, "inputs", crate=record["crate"])
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("checksum mismatch", result.stderr)
        finally:
            archive.write_bytes(original)
        result = verify(
            self.source, self.proof, "inputs", crate="svg-release-fixture-private"
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("absent from verified package set", result.stderr)

    def test_excluded_required_input_fails_only_the_packaged_build(self):
        directory = self.directory / "broken"
        directory.mkdir()
        source = fixture(directory, broken=True)
        execute(["cargo", "build", "--workspace", "--locked", "--offline"], source)
        proof = directory / "proof"
        proof.mkdir()
        (proof / "verification.json").write_bytes(self.original_report)
        result = verify(source, proof)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("required.txt", result.stdout)
        self.assertFalse(
            (proof / "verification.json").exists(),
            "a failed gate must leave no success report",
        )

    def test_checkout_revision_checks_bind_both_roles(self):
        output = self.directory / "identities-output"
        sha = execute(["git", "rev-parse", "HEAD"], self.source).stdout.strip()
        env = {**os.environ, "GITHUB_OUTPUT": str(output)}
        command = [
            sys.executable,
            str(IDENTITIES),
            "--source-dir",
            str(self.source),
            "--helper-dir",
            str(self.source),
        ]
        execute(
            [*command, "--expected-source", sha, "--expected-helper", sha],
            self.source,
            env=env,
        )
        self.assertIn(f"source_sha={sha}", output.read_text())
        self.assertIn(f"helper_sha={sha}", output.read_text())
        for role in ("source", "helper"):
            result = subprocess.run(
                [*command, f"--expected-{role}", "b" * 40],
                cwd=self.source,
                env=env,
                text=True,
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("changed since release verification", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
