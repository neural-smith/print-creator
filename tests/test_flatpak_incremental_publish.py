"""Verify two successive Flatpak Pages updates keep prior OSTree state and signing."""
from __future__ import annotations

import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/update-flatpak-pages.sh"
FINGERPRINT = "0123456789ABCDEF0123456789ABCDEF01234567"

# One shim for the external CLI programs, to test command sequencing without
# downloading a Flatpak SDK or creating actual OSTree commits in unit tests.
MOCK_TOOLS = r'''#!/usr/bin/env python3
import hashlib, os, pathlib, shutil, sys
argv = sys.argv[1:]
name = pathlib.Path(sys.argv[0]).name
with open(os.environ["COMMAND_LOG"], "a") as f:
    f.write(name + " " + " ".join(argv) + "\n")
if name == "gpg":
    if "--fingerprint" in argv:
        print("fpr:::::::::" + os.environ["MOCK_FINGERPRINT"] + ":")
    elif "--export" in argv:
        sys.stdout.buffer.write(b"MOCK_PUBLIC_KEY\n")
    sys.exit(0)
if name == "ostree":
    assert argv[0].startswith("--repo=")
    repo = pathlib.Path(argv[0][7:])
    cmd = argv[1]
    if cmd == "init":
        repo.mkdir(parents=True, exist_ok=True)
        (repo / "config").write_text("[core]\nmode=archive-z2\n")
        (repo / "refs/heads").mkdir(parents=True, exist_ok=True)
    elif cmd == "rev-parse":
        ref = argv[2]
        path = repo / "refs/heads" / ref
        if not path.is_file():
            sys.exit(1)
        print(path.read_text().strip())
    elif cmd == "pull-local":
        assert "--untrusted" in argv
        source = pathlib.Path(argv[-1])
        assert source != repo
        refs = list((source / "refs/heads").rglob("*"))
        for item in refs:
            if item.is_file():
                dst = repo / "refs/heads" / item.relative_to(source / "refs/heads")
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(item.read_bytes())
        old = repo / "mock-commit-history"
        present = set(old.read_text().splitlines()) if old.is_file() else set()
        present.add((repo / "refs/heads/app/org.neural_smith.PrintCreator/x86_64/master").read_text().strip())
        old.write_text("\n".join(sorted(present)))
    sys.exit(0)
if name == "flatpak":
    cmd = argv[0]
    if cmd == "build-import-bundle":
        assert "--gpg-sign=" + os.environ["MOCK_FINGERPRINT"] in argv
        assert "--update-appstream" in argv
        assert "--no-update-summary" in argv
        repo, bundle = pathlib.Path(argv[-2]), pathlib.Path(argv[-1])
        assert "print-creator-flatpak-import." in str(repo)
        assert (repo / "config").is_file()
        digest = hashlib.sha256(bundle.read_bytes()).hexdigest()
        for ref in ["app/org.neural_smith.PrintCreator/x86_64/master", "appstream/x86_64"]:
            dest = repo / "refs/heads" / ref
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(digest)
    elif cmd == "build-update-repo":
        assert "--gpg-sign=" + os.environ["MOCK_FINGERPRINT"] in argv
        repo = pathlib.Path(argv[-1])
        assert (repo / "refs/heads/app/org.neural_smith.PrintCreator/x86_64/master").is_file()
        (repo / "summary").write_text("current summary")
        (repo / "summary.sig").write_text("signed summary")
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
            stub = self.bin / name
            stub.write_text(MOCK_TOOLS)
            stub.chmod(0o755)
        self.output = self.base / "pages"
        self.output.mkdir()
        (self.output / "app").mkdir()
        (self.output / "app" / "index.html").write_text("keep web app")
        self.log = self.base / "commands.log"
        self.env = os.environ.copy()
        self.env.update({
            "PATH": str(self.bin) + os.pathsep + os.environ.get("PATH", ""),
            "FLATPAK_GPG_KEY_ID": FINGERPRINT,
            "MOCK_FINGERPRINT": FINGERPRINT,
            "COMMAND_LOG": str(self.log),
        })

    def publish(self, version):
        bundle = self.base / f"PrintCreator-{version}-x86_64.flatpak"
        bundle.write_bytes(f"bundle version {version}".encode())
        return subprocess.run(
            ["bash", str(SCRIPT), str(self.output), str(bundle),
             "https://neural-smith.github.io/print-creator/"],
            env=self.env, capture_output=True, text=True, timeout=30,
        )

    def test_successive_releases_preserve_history_signature_and_web_app(self):
        first = self.publish("0.5.0")
        self.assertEqual(first.returncode, 0, first.stderr)
        old_ref = (self.output / "refs/heads/app/org.neural_smith.PrintCreator/x86_64/master").read_text()
        second = self.publish("0.5.1")
        self.assertEqual(second.returncode, 0, second.stderr)
        new_ref = (self.output / "refs/heads/app/org.neural_smith.PrintCreator/x86_64/master").read_text()
        self.assertNotEqual(old_ref, new_ref)
        self.assertEqual(len((self.output / "mock-commit-history").read_text().splitlines()), 2)
        self.assertEqual((self.output / "flatpak-signing-fingerprint.txt").read_text().strip(), FINGERPRINT)
        self.assertTrue((self.output / "summary.sig").is_file())
        self.assertTrue((self.output / "index.flatpakrepo").is_file())
        self.assertEqual((self.output / "app/index.html").read_text(), "keep web app")
        logs = self.log.read_text().splitlines()
        self.assertEqual(sum("flatpak build-import-bundle " in s for s in logs), 2)
        self.assertEqual(sum("ostree " in s and " pull-local " in s for s in logs), 2)
        self.assertEqual(sum("flatpak build-update-repo " in s for s in logs), 2)
        self.assertFalse(any("build-import-bundle" in s and str(self.output) in s for s in logs))

    def test_signing_key_rotation_rejected_without_mutating_repo(self):
        result = self.publish("0.5.0")
        self.assertEqual(result.returncode, 0, result.stderr)
        previous = (self.output / "refs/heads/app/org.neural_smith.PrintCreator/x86_64/master").read_text()
        self.env["MOCK_FINGERPRINT"] = "F" * 40
        result = self.publish("0.5.1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Signing key changed", result.stderr)
        self.assertEqual((self.output / "refs/heads/app/org.neural_smith.PrintCreator/x86_64/master").read_text(), previous)


if __name__ == "__main__":
    unittest.main()
