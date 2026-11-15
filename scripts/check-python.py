#!/usr/bin/env python3
"""Check analysis targets and exercise all four installed grammar wheels."""

import importlib.metadata
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GRAMMARS = sorted((ROOT / "grammars").glob("tree-sitter-svg*"))


def run(*args: str) -> None:
    subprocess.run(args, cwd=ROOT, check=True)


def metadata_check() -> None:
    for grammar in GRAMMARS:
        project = tomllib.loads((grammar / "pyproject.toml").read_text())["project"]
        metadata = importlib.metadata.metadata(project["name"])
        assert metadata["Requires-Python"] == project["requires-python"], grammar
        assert metadata["Version"] == project["version"], grammar
        print(f"Installed wheel metadata verified: {project['name']}", flush=True)


def main() -> None:
    if sys.argv[1:] == ["--installed"]:
        metadata_check()
        return
    expected = (ROOT / ".python-version").read_text().strip()
    actual = f"{sys.version_info.major}.{sys.version_info.minor}"
    assert actual == expected, (actual, expected)
    assert len(GRAMMARS) == 4
    for grammar in GRAMMARS:
        project = tomllib.loads((grammar / "pyproject.toml").read_text())["project"]
        assert project["requires-python"] == f">={expected}", grammar
    run("uv", "run", "--locked", "--no-sync", "ruff", "check")
    run("uv", "run", "--locked", "--no-sync", "basedpyright")
    run("uv", "run", "--locked", "--no-sync", "ty", "check")
    lock = tomllib.loads((ROOT / "uv.lock").read_text())
    tree_sitter = next(p for p in lock["package"] if p["name"] == "tree-sitter")
    with tempfile.TemporaryDirectory(prefix="svg-python-wheels-") as directory:
        temp = Path(directory)
        wheels = temp / "wheels"
        for grammar in GRAMMARS:
            run(
                "uv",
                "build",
                "--wheel",
                "--package",
                grammar.name,
                "--out-dir",
                str(wheels),
            )
        artifacts = sorted(wheels.glob("*.whl"))
        assert len(artifacts) == 4, artifacts
        environment = temp / "venv"
        run("uv", "venv", "--python", sys.executable, str(environment))
        python = environment / (
            "Scripts/python.exe" if sys.platform == "win32" else "bin/python"
        )
        run(
            "uv",
            "pip",
            "install",
            "--python",
            str(python),
            f"tree-sitter=={tree_sitter['version']}",
            *(str(wheel) for wheel in artifacts),
        )
        run(str(python), "-I", str(Path(__file__).resolve()), "--installed")
        for grammar in GRAMMARS:
            run(
                str(python),
                "-I",
                "-m",
                "unittest",
                "discover",
                "-v",
                "-s",
                str(grammar / "bindings/python/tests"),
            )


if __name__ == "__main__":
    main()
