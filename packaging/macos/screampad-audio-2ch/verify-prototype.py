#!/usr/bin/env python3
"""Inspect an unsigned ScreamPad Audio prototype package without installing it."""

from __future__ import annotations

import plistlib
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from pathlib import Path

BUNDLE = "ScreamPadAudio2ch.driver"
BUNDLE_ID = "io.github.golovaschukwork.screampad.audio.driver"
PACKAGE_ID = "io.github.golovaschukwork.screampad.audio.pkg.2ch"
VERSION = "0.7.1.1"
FACTORY = "DF08D44A-FEDF-5AC4-9796-F10DDC9A1970"
TYPE = "443ABAB8-E7B3-491A-B985-BEB9187030DB"
UIDS = {
    "io.github.golovaschukwork.screampad.audio.device.2ch",
    "io.github.golovaschukwork.screampad.audio.box.2ch",
    "io.github.golovaschukwork.screampad.audio.model.2ch",
    "io.github.golovaschukwork.screampad.audio.device.2ch.mirror",
}


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def verify_bundle(bundle: Path) -> None:
    info = plistlib.loads((bundle / "Contents/Info.plist").read_bytes())
    require(info["CFBundleIdentifier"] == BUNDLE_ID, "Wrong plug-in bundle ID")
    require(info["CFBundleName"] == "ScreamPadAudio2ch", "Wrong bundle name")
    require(info["CFBundleExecutable"] == "ScreamPadAudio2ch", "Wrong executable name")
    require(info["CFBundleShortVersionString"] == VERSION, "Wrong bundle version")
    require(set(info["CFPlugInFactories"]) == {FACTORY}, "Wrong factory UUID")
    require(info["CFPlugInTypes"][TYPE] == [FACTORY], "Wrong plug-in type mapping")

    executable = bundle / "Contents/MacOS/ScreamPadAudio2ch"
    binary = executable.read_bytes()
    for marker in UIDS | {BUNDLE_ID, "ScreamPad Audio", "ScreamPadAudio.icns"}:
        require(marker.encode() in binary, f"Missing binary identity: {marker}")
    for marker in (b"BlackHole2ch_UID", b"audio.existential.BlackHole2ch"):
        require(marker not in binary, f"Upstream identity remains in binary: {marker!r}")
    require((bundle / "Contents/Resources/ScreamPadAudio.icns").is_file(), "Missing owned icon")
    require((bundle / "Contents/Resources/LICENSE").is_file(), "Missing GPL license")
    require((bundle / "Contents/Resources/ScreamPad-ATTRIBUTION.txt").is_file(), "Missing attribution")
    architectures = subprocess.check_output(["lipo", "-archs", str(executable)], text=True).split()
    require(set(architectures) == {"arm64", "x86_64"}, "Missing universal architecture")


def verify_expanded_package(root: Path) -> None:
    package_info = ET.parse(root / "PackageInfo").getroot()
    require(package_info.attrib["identifier"] == PACKAGE_ID, "Wrong package/receipt ID")
    require(package_info.attrib["version"] == VERSION, "Wrong package version")
    require(package_info.attrib["install-location"] == "/Library/Audio/Plug-Ins/HAL", "Wrong install location")
    require(not (root / "Scripts").exists(), "Installer scripts are forbidden")
    require(set(root.iterdir()) == {root / "PackageInfo", root / "Bom", root / "Payload"}, "Unexpected package structure")
    payload_root = root / "Payload"
    payload = payload_root / BUNDLE
    require(payload.is_dir(), "Expected driver bundle is missing")
    require(set(payload_root.iterdir()) == {payload}, "Unexpected package payload")
    require(not any(item.is_symlink() for item in payload.rglob("*")), "Symlinks in payload are forbidden")
    require(not any(item.name.startswith("._") for item in payload.rglob("*")), "AppleDouble metadata in payload")
    verify_bundle(payload)


def verify_payload_listing(lines: list[str]) -> None:
    """Accept AppleDouble metadata only when it belongs to the owned bundle."""
    require(bool(lines), "Empty package payload listing")
    for entry in lines:
        require(entry == "." or entry.startswith("./"), f"Unexpected payload path: {entry}")
        if entry == ".":
            continue
        parts = Path(entry).parts
        require(".." not in parts, f"Parent traversal in payload: {entry}")
        first = parts[0][2:] if parts[0].startswith("._") else parts[0]
        require(first == BUNDLE, f"Other provider or path in payload: {entry}")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("Usage: verify-prototype.py <unsigned package>")
    package = Path(sys.argv[1]).resolve(strict=True)
    listing = subprocess.check_output(["pkgutil", "--payload-files", str(package)], text=True).splitlines()
    verify_payload_listing(listing)
    with tempfile.TemporaryDirectory(prefix="screampad-audio-inspect-") as temporary:
        expanded = Path(temporary) / "expanded"
        subprocess.run(["pkgutil", "--expand-full", str(package), str(expanded)], check=True)
        verify_expanded_package(expanded)
    print("PASS: unsigned package identity, owned payload, bundle, UIDs, attribution, and architectures")


if __name__ == "__main__":
    main()
