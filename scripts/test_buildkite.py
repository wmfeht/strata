# SPDX-License-Identifier: MIT
"""Buildkite pipeline wiring, without mise, Cargo, Docker, or network access."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[1]
PIPELINE = REPOSITORY / ".buildkite/pipeline.yml"
RUNNER = REPOSITORY / ".buildkite/run.sh"
MISE = REPOSITORY / "mise.toml"
DOCKERFILE = REPOSITORY / "tests/e2e/Dockerfile"
CI = REPOSITORY / ".github/workflows/ci.yml"


class PipelineTests(unittest.TestCase):
    def test_pipeline_declares_the_github_ci_gates(self):
        text = PIPELINE.read_text()
        for key in ("fmt", "clippy", "test", "scripts", "deny", "typos", "e2e"):
            self.assertIn(f"key: {key}", text)
        self.assertIn(".buildkite/run.sh ./scripts/quality.sh fmt", text)
        self.assertIn(".buildkite/run.sh ./scripts/quality.sh clippy", text)
        self.assertIn(".buildkite/run.sh ./scripts/quality.sh test", text)
        self.assertIn(".buildkite/run.sh python3 -m unittest discover -s scripts -p 'test_*.py'", text)
        self.assertIn(".buildkite/run.sh cargo deny check", text)
        self.assertIn(".buildkite/run.sh typos", text)
        self.assertIn(".buildkite/run.sh ./scripts/e2e.sh", text)
        self.assertIn("STRATA_CONTAINER_ENGINE: docker", text)
        self.assertIn("STRATA_E2E_WORKERS: \"8\"", text)
        self.assertIn("target/e2e-artifacts/**/*", text)
        self.assertNotIn("nix develop", text)
        self.assertNotIn("flake.nix", text)

    def test_shell_scripts_have_valid_syntax(self):
        result = subprocess.run(["bash", "-n", str(RUNNER)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)


class MiseTests(unittest.TestCase):
    def test_mise_pins_the_ci_toolchain(self):
        text = MISE.read_text()
        self.assertIn('rust = { version = "1.98.1"', text)
        self.assertIn('"aqua:EmbarkStudios/cargo-deny" = "0.20.2"', text)
        self.assertIn('"aqua:crate-ci/typos" = "1.50.1"', text)
        self.assertIn('run = "cargo fmt --all --check"', text)
        self.assertIn('run = "cargo clippy --all-targets --all-features --locked -- -D warnings"', text)
        self.assertIn('run = "cargo deny check"', text)
        self.assertIn('run = "typos"', text)
        self.assertIn("RUSTUP_TOOLCHAIN=1.98.1", DOCKERFILE.read_text())


class GitHubCiTests(unittest.TestCase):
    def test_quality_job_uses_the_published_container(self):
        quality = CI.read_text().split("e2e-build:")[0]
        self.assertIn("./scripts/quality.sh fmt", quality)
        self.assertIn("./scripts/quality.sh clippy", quality)
        self.assertIn("./scripts/quality.sh test", quality)
        self.assertNotIn("nix develop", quality)
        self.assertNotIn("dtolnay/rust-toolchain", quality)

    def test_policy_job_uses_pinned_actions(self):
        policy = CI.read_text().split("repository-policy:")[1]
        self.assertIn("EmbarkStudios/cargo-deny-action", policy)
        self.assertIn("crate-ci/typos", policy)
        self.assertNotIn("DeterminateSystems/determinate-nix-action", policy)


class ToolchainRunnerTests(unittest.TestCase):
    def run_runner(self, *command, with_mise=True):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "calls.jsonl"
            environment = {**os.environ, "ENGINE_LOG": str(log)}
            for name in (
                "CARGO_HOME",
                "CARGO_TARGET_DIR",
                "STRATA_CONTAINER_ENGINE",
                "MISE_YES",
                "MISE_DATA_DIR",
                "MISE_CACHE_DIR",
            ):
                environment.pop(name, None)
            shutil.copy("/bin/bash", root / "bash")
            system_path = os.pathsep.join(("/usr/bin", "/bin"))
            if with_mise:
                mise = root / "mise"
                mise.write_text(
                    f"#!{sys.executable}\n"
                    "import json, os, sys\n"
                    "with open(os.environ['ENGINE_LOG'], 'a') as stream:\n"
                    "    stream.write(json.dumps({\n"
                    "        'argv': sys.argv[1:],\n"
                    "        'cwd': os.getcwd(),\n"
                    "        'cargo_home': os.environ.get('CARGO_HOME'),\n"
                    "        'cargo_target_dir': os.environ.get('CARGO_TARGET_DIR'),\n"
                    "        'container_engine': os.environ.get('STRATA_CONTAINER_ENGINE'),\n"
                    "        'mise_yes': os.environ.get('MISE_YES'),\n"
                    "        'mise_data_dir': os.environ.get('MISE_DATA_DIR'),\n"
                    "        'mise_cache_dir': os.environ.get('MISE_CACHE_DIR'),\n"
                    "    }) + '\\n')\n"
                )
                mise.chmod(0o755)
                environment["PATH"] = f"{root}{os.pathsep}{system_path}"
            else:
                environment["PATH"] = str(root)
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

    def test_run_installs_and_execs_through_mise(self):
        result, calls = self.run_runner("cargo", "fmt", "--all", "--check")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0]["argv"], ["install"])
        self.assertEqual(calls[1]["argv"][:2], ["exec", "--"])
        self.assertEqual(calls[1]["argv"][-4:], ["cargo", "fmt", "--all", "--check"])
        self.assertEqual(calls[1]["cwd"], str(REPOSITORY))
        self.assertTrue(calls[0]["cargo_home"].endswith("target/buildkite/cargo"))
        self.assertTrue(calls[0]["cargo_target_dir"].endswith("target/buildkite/build"))
        self.assertEqual(calls[0]["container_engine"], "docker")
        self.assertEqual(calls[0]["mise_yes"], "1")
        self.assertTrue(calls[0]["mise_data_dir"].endswith("target/buildkite/mise"))
        self.assertTrue(calls[0]["mise_cache_dir"].endswith("target/buildkite/mise-cache"))

    def test_missing_mise_fails_without_running_a_command(self):
        result, calls = self.run_runner("cargo", "fmt", with_mise=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])
        self.assertIn("mise", result.stderr)
