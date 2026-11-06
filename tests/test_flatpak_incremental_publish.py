"""Regression for signed, successive Pages releases without bundle re-import."""
import hashlib
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/update-flatpak-pages.sh"
FINGERPRINT = "0123456789ABCDEF0123456789ABCDEF01234567"
APP_REF = "app/org.neural_smith.PrintCreator/x86_64/master"

MOCK_TOOLS = r'''#!/usr/bin/env python3
import os, pathlib, sys
args = sys.argv[1:]
name = pathlib.Path(sys.argv[0]).name
with open(os.environ["COMMAND_LOG"], "a") as f:
    f.write(name + " " + " ".join(args) + "\n")
if name == "gpg":
    if "--fingerprint" in args:
        print("fpr:::::::::" + os.environ["MOCK_FINGERPRINT"] + ":")
    elif "--export" in args:
        sys.stdout.buffer.write(b"PUBLIC_KEY\n")
    sys.exit(0)
if name == "ostree":
    assert args[0].startswith("--repo=")
    repo = pathlib.Path(args[0][7:])
    cmd = args[1]
    if cmd == "init":
        repo.mkdir(parents=True, exist_ok=True)
        (repo / "config").write_text("[core]\nmode=archive-z2\n")
        (repo / "refs/heads").mkdir(parents=True, exist_ok=True)
    elif cmd == "rev-parse":
        ref = args[2]
        target = repo / "refs/heads" / ref
        if not target.is_file():
            sys.exit(1)
        print(target.read_text().strip())
    elif cmd == "pull-local":
        assert args[2] == "--untrusted"
        source, ref = pathlib.Path(args[3]), args[4]
        assert source != repo
        source_ref = source / "refs/heads" / ref
        assert source_ref.is_file(), source_ref
        target = repo / "refs/heads" / ref
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(source_ref.read_bytes())
        previous = repo / "mock-commit-history"
        seen = set(previous.read_text().splitlines()) if previous.is_file() else set()
        seen.add(target.read_text().strip())
        previous.write_text("\n".join(sorted(seen)))
    sys.exit(0)
if name == "flatpak":
    cmd = args[0]
    if cmd == "build-sign":
        assert "--gpg-sign=" + os.environ["MOCK_FINGERPRINT"] in args
        repo = pathlib.Path(args[-3])
        assert (repo / "refs/heads/app/org.neural_smith.PrintCreator/x86_64/master").is_file()
        (repo / "mock-commit-signature").write_text(os.environ["MOCK_FINGERPRINT"])
    elif cmd == "build-update-repo":
        assert "--gpg-sign=" + os.environ["MOCK_FINGERPRINT"] in args
        repo = pathlib.Path(args[-1])
        # The missing directory triggered the actual failure on the second release.
        assert (repo / "refs/remotes").is_dir()
        assert (repo / "mock-commit-signature").read_text() == os.environ["MOCK_FINGERPRINT"]
        (repo / "summary").write_text("current summary")
        (repo / "summary.sig").write_text("signed summary")
    else:
        raise AssertionError("Unexpected Flatpak command: " + cmd)
    sys.exit(0)
raise AssertionError(name)
'''


class FlatpakIncrementalPublishTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.bin = self.base / "bin"
        self.bin.mkdir()
        for name in ("flatpak", "ostree", "gpg"):
            executable = self.bin / name
            executable.write_text(MOCK_TOOLS)
            executable.chmod(0o755)
        self.pages = self.base / "pages"
        (self.pages / "app").mkdir(parents=True)
        (self.pages / "app/index.html").write_text("keep browser app")
        self.log = self.base / "commands.log"
        self.env = dict(os.environ,
            PATH=str(self.bin) + os.pathsep + os.environ.get("PATH", ""),
            FLATPAK_GPG_KEY_ID=FINGERPRINT, MOCK_FINGERPRINT=FINGERPRINT,
            COMMAND_LOG=str(self.log))

    def publish(self, version):
        built = self.base / f"built-{version}"
        app = built / "refs/heads" / APP_REF
        app.parent.mkdir(parents=True)
        app.write_text(hashlib.sha256(version.encode()).hexdigest())
        (built / "config").write_text("[core]\nmode=archive-z2\n")
        return subprocess.run(["bash", str(SCRIPT), str(self.pages), str(built),
            "https://neural-smith.github.io/print-creator/"],
            env=self.env, text=True, capture_output=True, timeout=30)

    def test_two_releases_keep_old_objects_signing_key_and_browser(self):
        first = self.publish("0.5.0")
        self.assertEqual(first.returncode, 0, first.stderr)
        old = (self.pages / "refs/heads" / APP_REF).read_text()
        # Simulates the empty-directory loss when gh-pages was restored via Git.
        (self.pages / "refs/remotes").rmdir()
        second = self.publish("0.5.1")
        self.assertEqual(second.returncode, 0, second.stderr)
        current = (self.pages / "refs/heads" / APP_REF).read_text()
        self.assertNotEqual(old, current)
        self.assertEqual(len((self.pages / "mock-commit-history").read_text().splitlines()), 2)
        self.assertEqual((self.pages / "flatpak-signing-fingerprint.txt").read_text().strip(), FINGERPRINT)
        self.assertEqual((self.pages / "app/index.html").read_text(), "keep browser app")
        self.assertTrue((self.pages / "summary.sig").is_file())
        log = self.log.read_text()
        self.assertNotIn("build-import-bundle", log)
        self.assertEqual(log.count("ostree --repo=" + str(self.pages) + " pull-local "), 2)
        self.assertEqual(log.count("flatpak build-sign "), 2)

    def test_key_rotation_rejected_before_mutation(self):
        result = self.publish("0.5.0")
        self.assertEqual(result.returncode, 0, result.stderr)
        old = (self.pages / "refs/heads" / APP_REF).read_text()
        self.env["MOCK_FINGERPRINT"] = "F" * 40
        result = self.publish("0.5.1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Signing key changed", result.stderr)
        self.assertEqual((self.pages / "refs/heads" / APP_REF).read_text(), old)

    def test_invalid_source_rejected_before_touching_pages(self):
        built = self.base / "empty-built"
        built.mkdir()
        (built / "config").write_text("[core]")
        result = subprocess.run(["bash", str(SCRIPT), str(self.pages), str(built),
            "https://neural-smith.github.io/print-creator/"],
            env=self.env, text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.pages / "config").exists())


if __name__ == "__main__":
    unittest.main()
