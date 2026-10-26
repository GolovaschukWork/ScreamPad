"""Negative-path checks for the unsigned package inspector."""

from __future__ import annotations

import importlib.util
import plistlib
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("verify-prototype.py")
SPEC = importlib.util.spec_from_file_location("verify_prototype", SCRIPT)
assert SPEC and SPEC.loader
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)


class PackageInspectionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        ET.ElementTree(
            ET.Element(
                "pkg-info",
                {
                    "identifier": verifier.PACKAGE_ID,
                    "version": verifier.VERSION,
                    "install-location": "/Library/Audio/Plug-Ins/HAL",
                },
            )
        ).write(self.root / "PackageInfo")
        (self.root / "Bom").touch()
        bundle = self.root / "Payload" / verifier.BUNDLE
        (bundle / "Contents/MacOS").mkdir(parents=True)
        (bundle / "Contents/Resources").mkdir()
        info = {
            "CFBundleIdentifier": verifier.BUNDLE_ID,
            "CFBundleName": "ScreamPadAudio2ch",
            "CFBundleExecutable": "ScreamPadAudio2ch",
            "CFBundleShortVersionString": verifier.VERSION,
            "CFPlugInFactories": {verifier.FACTORY: "BlackHole_Create"},
            "CFPlugInTypes": {verifier.TYPE: [verifier.FACTORY]},
        }
        (bundle / "Contents/Info.plist").write_bytes(plistlib.dumps(info))
        markers = verifier.UIDS | {verifier.BUNDLE_ID, "ScreamPad Audio", "ScreamPadAudio.icns"}
        (bundle / "Contents/MacOS/ScreamPadAudio2ch").write_bytes(b"\0".join(m.encode() for m in markers))
        for resource in ("ScreamPadAudio.icns", "LICENSE", "ScreamPad-ATTRIBUTION.txt"):
            (bundle / "Contents/Resources" / resource).touch()

    def verify(self) -> None:
        with patch.object(verifier.subprocess, "check_output", return_value="arm64 x86_64"):
            verifier.verify_expanded_package(self.root)

    def test_expected_package_is_accepted(self) -> None:
        self.verify()

    def test_original_blackhole_bundle_identity_is_rejected(self) -> None:
        plist = self.root / "Payload" / verifier.BUNDLE / "Contents/Info.plist"
        info = plistlib.loads(plist.read_bytes())
        info["CFBundleIdentifier"] = "audio.existential.BlackHole2ch"
        plist.write_bytes(plistlib.dumps(info))
        with self.assertRaisesRegex(ValueError, "Wrong plug-in bundle ID"):
            self.verify()

    def test_wrong_receipt_identity_is_rejected(self) -> None:
        path = self.root / "PackageInfo"
        tree = ET.parse(path)
        tree.getroot().set("identifier", "audio.existential.BlackHole2ch")
        tree.write(path)
        with self.assertRaisesRegex(ValueError, "Wrong package/receipt ID"):
            self.verify()

    def test_installer_scripts_are_rejected(self) -> None:
        (self.root / "Scripts").mkdir()
        with self.assertRaisesRegex(ValueError, "Installer scripts are forbidden"):
            self.verify()

    def test_extra_provider_payload_is_rejected(self) -> None:
        (self.root / "Payload/BlackHole2ch.driver").mkdir()
        with self.assertRaisesRegex(ValueError, "Unexpected package payload"):
            self.verify()

    def test_appledouble_metadata_is_rejected(self) -> None:
        (self.root / "Payload" / verifier.BUNDLE / "._Contents").touch()
        with self.assertRaisesRegex(ValueError, "AppleDouble metadata in payload"):
            self.verify()

    def test_archive_metadata_for_owned_bundle_is_accepted(self) -> None:
        verifier.verify_payload_listing([".", "./ScreamPadAudio2ch.driver", "./._ScreamPadAudio2ch.driver"])

    def test_archive_path_for_other_provider_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "Other provider or path"):
            verifier.verify_payload_listing([".", "./ScreamPadAudio2ch.driver", "./BlackHole2ch.driver"])


if __name__ == "__main__":
    unittest.main()
