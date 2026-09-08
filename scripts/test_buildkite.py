"""Buildkite pipeline wiring, without Docker, Cargo, or network access."""

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
POLICY = REPOSITORY / ".buildkite/policy.sh"


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
        self.assertIn(".buildkite/policy.sh deny", text)
        self.assertIn(".buildkite/policy.sh typos", text)
        self.assertIn("./scripts/e2e.sh", text)
        self.assertIn("target/e2e-artifacts/**/*", text)

    def test_shell_scripts_have_valid_syntax(self):
        for script in (RUNNER, POLICY):
            with self.subTest(script=script.name):
                result = subprocess.run(["bash", "-n", str(script)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)


class ToolchainRunnerTests(unittest.TestCase):
    def run_runner(self, *command, engine_name="docker", uid=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            engine = root / engine_name
            log = root / "calls.jsonl"
            engine.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['ENGINE_LOG'], 'a') as stream:\n"
                "    stream.write(json.dumps(sys.argv[1:]) + '\\n')\n"
            )
            engine.chmod(0o755)
            environment = {
                **os.environ,
                "STRATA_CONTAINER_ENGINE": str(engine),
                "ENGINE_LOG": str(log),
            }
            if uid is not None:
                identity = root / "id"
                identity.write_text(f"#!/bin/sh\necho {uid}\n")
                identity.chmod(0o755)
                environment["PATH"] = f"{root}:{os.environ.get('PATH', os.defpath)}"
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

    def test_build_and_run_share_the_e2e_toolchain_image(self):
        result, calls = self.run_runner("cargo", "fmt", "--all", "--check")
        self.assertEqual(result.returncode, 0, result.stderr)
        build, run = calls
        self.assertEqual(build[0], "build")
        self.assertIn("--target", build)
        self.assertEqual(build[build.index("--target") + 1], "toolchain")
        self.assertIn(str(REPOSITORY / "tests/e2e/Dockerfile"), build)
        image = build[build.index("--tag") + 1]
        self.assertTrue(image.startswith("strata-e2e:"))
        self.assertIn(image, run)
        self.assertEqual(run[0], "run")
        self.assertIn(f"type=bind,source={REPOSITORY},target=/workspace", run)
        self.assertIn("CARGO_HOME=/workspace/target/buildkite/cargo", run)
        self.assertIn("CARGO_TARGET_DIR=/workspace/target/buildkite/build", run)
        self.assertEqual(run[-4:], ["cargo", "fmt", "--all", "--check"])
        self.assertNotIn("--userns=keep-id", run)

    def test_rootless_podman_preserves_checkout_ownership(self):
        result, calls = self.run_runner("cargo", "fmt", "--all", "--check", engine_name="podman")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--userns=keep-id", calls[1])
        self.assertIn("--passwd=false", calls[1])

    def test_non_default_uid_has_a_matching_image_account(self):
        result, calls = self.run_runner("true", uid=1001)
        self.assertEqual(result.returncode, 0, result.stderr)
        build, run = calls
        self.assertIn("E2E_UID=1001", build)
        self.assertIn("E2E_GID=1001", build)
        self.assertTrue(build[build.index("--tag") + 1].endswith("-1001-1001"))
        self.assertEqual(run[run.index("--user") + 1], "1001:1001")


class PolicyToolTests(unittest.TestCase):
    def run_policy(self, tool, arch="x86_64", curl_status=0):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / "calls.jsonl"
            extracted = root / "extracted"
            extracted.mkdir()
            curl = root / "curl"
            tar = root / "tar"
            uname = root / "uname"
            curl.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "with open(os.environ['TOOL_LOG'], 'a') as stream:\n"
                "    stream.write(json.dumps(['curl', sys.argv[1:]]) + '\\n')\n"
                "sys.exit(int(os.environ['CURL_STATUS']))\n"
            )
            tar.write_text(
                f"#!{sys.executable}\n"
                "import json, os, sys\n"
                "from pathlib import Path\n"
                "with open(os.environ['TOOL_LOG'], 'a') as stream:\n"
                "    stream.write(json.dumps(['tar', sys.argv[1:]]) + '\\n')\n"
                "dest = Path(sys.argv[sys.argv.index('-C') + 1])\n"
                "if os.environ['POLICY_TOOL'] == 'deny':\n"
                "    version = os.environ['DENY_VERSION']\n"
                "    triple = os.environ['TRIPLE']\n"
                "    binary = dest / f'cargo-deny-{version}-{triple}' / 'cargo-deny'\n"
                "    binary.parent.mkdir(parents=True)\n"
                "    binary.write_text('#!/bin/sh\\necho deny\\n')\n"
                "    binary.chmod(0o755)\n"
                "else:\n"
                "    binary = dest / 'typos'\n"
                "    binary.write_text('#!/bin/sh\\necho typos\\n')\n"
                "    binary.chmod(0o755)\n"
            )
            uname.write_text(f"#!/bin/sh\necho {arch}\n")
            for path in (curl, tar, uname):
                path.chmod(0o755)
            triple = {
                "x86_64": "x86_64-unknown-linux-musl",
                "aarch64": "aarch64-unknown-linux-musl",
                "arm64": "aarch64-unknown-linux-musl",
            }.get(arch, "unused")
            environment = {
                **os.environ,
                "PATH": f"{root}:{os.environ.get('PATH', os.defpath)}",
                "TOOL_LOG": str(log),
                "CURL_STATUS": str(curl_status),
                "POLICY_TOOL": tool or "",
                "DENY_VERSION": "0.20.2",
                "TRIPLE": triple,
            }
            result = subprocess.run(
                [str(POLICY), tool] if tool is not None else [str(POLICY)],
                env=environment,
                capture_output=True,
                text=True,
                check=False,
            )
            calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
            return result, calls

    def test_usage_without_a_tool(self):
        result, calls = self.run_policy(None)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(calls, [])
        self.assertIn("usage:", result.stderr)

    def test_deny_downloads_the_pinned_musl_archive(self):
        result, calls = self.run_policy("deny")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "deny\n")
        url = calls[0][1][-1]
        self.assertEqual(
            url,
            "https://github.com/EmbarkStudios/cargo-deny/releases/download/"
            "0.20.2/cargo-deny-0.20.2-x86_64-unknown-linux-musl.tar.gz",
        )

    def test_typos_downloads_the_pinned_musl_archive(self):
        result, calls = self.run_policy("typos")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "typos\n")
        url = calls[0][1][-1]
        self.assertEqual(
            url,
            "https://github.com/crate-ci/typos/releases/download/"
            "v1.50.1/typos-v1.50.1-x86_64-unknown-linux-musl.tar.gz",
        )

    def test_aarch64_selects_the_matching_musl_triple(self):
        result, calls = self.run_policy("typos", arch="aarch64")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("aarch64-unknown-linux-musl", calls[0][1][-1])

    def test_unsupported_architecture_does_not_download(self):
        result, calls = self.run_policy("deny", arch="ppc64le")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])
        self.assertIn("unsupported architecture", result.stderr)
