"""Buildkite pipeline wiring, without Nix, Cargo, or network access."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[1]
PIPELINE = REPOSITORY / ".buildkite/pipeline.yml"
RUNNER = REPOSITORY / ".buildkite/run.sh"
FLAKE = REPOSITORY / "flake.nix"
LOCK = REPOSITORY / "flake.lock"
CI = REPOSITORY / ".github/workflows/ci.yml"


class PipelineTests(unittest.TestCase):
    def test_pipeline_declares_the_github_ci_gates(self):
        text = PIPELINE.read_text()
        for key in ("fmt", "clippy", "test", "scripts", "deny", "typos", "e2e"):
            self.assertIn(f"key: {key}", text)
        self.assertIn(".buildkite/run.sh cargo fmt --all --check", text)
        self.assertIn("cargo clippy --all-targets --all-features --locked", text)
        self.assertIn("cargo test --all-targets --all-features --locked", text)
        self.assertIn("env -u DISPLAY -u WAYLAND_DISPLAY GDK_BACKEND=x11", text)
        self.assertIn("python3 -m unittest discover -s scripts -p 'test_*.py'", text)
        self.assertIn(".buildkite/run.sh cargo deny check", text)
        self.assertIn(".buildkite/run.sh typos", text)
        self.assertIn("./scripts/e2e.sh", text)
        self.assertIn("target/e2e-artifacts/**/*", text)

    def test_shell_scripts_have_valid_syntax(self):
        result = subprocess.run(["bash", "-n", str(RUNNER)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)


class FlakeTests(unittest.TestCase):
    def test_flake_pins_the_e2e_rust_toolchain(self):
        text = FLAKE.read_text()
        self.assertIn('rustVersion = "1.98.1"', text)
        self.assertIn("pkgs.gtk4", text)
        self.assertIn("pkgs.gtksourceview5", text)
        self.assertIn("pkgs.poppler", text)
        self.assertIn("pkgs.fontconfig", text)
        self.assertIn("pkgs.cargo-deny", text)
        self.assertIn("pkgs.typos", text)

    def test_lockfile_pins_nixpkgs_and_rust_overlay(self):
        lock = json.loads(LOCK.read_text())
        self.assertEqual(lock["nodes"]["nixpkgs"]["original"]["ref"], "nixos-unstable")
        self.assertEqual(lock["nodes"]["rust-overlay"]["original"]["owner"], "oxalica")
        self.assertEqual(lock["nodes"]["root"]["inputs"]["rust-overlay"], "rust-overlay")


class GitHubCiTests(unittest.TestCase):
    def test_quality_job_enters_nix_develop(self):
        quality = CI.read_text().split("e2e-build:")[0]
        self.assertIn("DeterminateSystems/determinate-nix-action", quality)
        self.assertIn("nicknovitski/nix-develop", quality)
        self.assertNotIn("apt-get", quality)
        self.assertNotIn("dtolnay/rust-toolchain", quality)

    def test_policy_job_uses_the_flake_tools(self):
        policy = CI.read_text().split("repository-policy:")[1]
        self.assertIn("DeterminateSystems/determinate-nix-action", policy)
        self.assertIn("nicknovitski/nix-develop", policy)
        self.assertIn("cargo deny check", policy)
        self.assertIn("run: typos", policy)
        self.assertNotIn("EmbarkStudios/cargo-deny-action", policy)
        self.assertNotIn("crate-ci/typos", policy)


class ToolchainRunnerTests(unittest.TestCase):
    def run_runner(self, *command, with_nix=True):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "calls.jsonl"
            environment = {**os.environ, "ENGINE_LOG": str(log)}
            environment.pop("CARGO_HOME", None)
            environment.pop("CARGO_TARGET_DIR", None)
            system_path = os.pathsep.join(("/usr/bin", "/bin"))
            if with_nix:
                nix = root / "nix"
                nix.write_text(
                    f"#!{sys.executable}\n"
                    "import json, os, sys\n"
                    "with open(os.environ['ENGINE_LOG'], 'a') as stream:\n"
                    "    stream.write(json.dumps({\n"
                    "        'argv': sys.argv[1:],\n"
                    "        'cargo_home': os.environ.get('CARGO_HOME'),\n"
                    "        'cargo_target_dir': os.environ.get('CARGO_TARGET_DIR'),\n"
                    "    }) + '\\n')\n"
                )
                nix.chmod(0o755)
                environment["PATH"] = f"{root}{os.pathsep}{system_path}"
            else:
                environment["PATH"] = system_path
            result = subprocess.run(
                [str(RUNNER), *command],
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            return result, calls

    def test_usage_without_a_command(self):
        result, calls = self.run_runner()
        self.assertEqual(result.returncode, 2)
        self.assertEqual(calls, [])
        self.assertIn("usage:", result.stderr)

    def test_run_enters_the_flake_development_shell(self):
        result, calls = self.run_runner("cargo", "fmt", "--all", "--check")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 1)
        argv = calls[0]["argv"]
        self.assertEqual(argv[:3], ["--extra-experimental-features", "nix-command flakes", "develop"])
        self.assertEqual(argv[3], "--command")
        self.assertEqual(argv[-4:], ["cargo", "fmt", "--all", "--check"])
        self.assertTrue(calls[0]["cargo_home"].endswith("target/buildkite/cargo"))
        self.assertTrue(calls[0]["cargo_target_dir"].endswith("target/buildkite/build"))

    def test_missing_nix_fails_without_running_a_command(self):
        result, calls = self.run_runner("cargo", "fmt", with_nix=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])
        self.assertIn("Nix", result.stderr)
