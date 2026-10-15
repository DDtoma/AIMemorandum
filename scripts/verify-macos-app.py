"""Check a real macOS application bundle, without granting privacy permissions."""
import json
import os
import plistlib
from pathlib import Path
import subprocess
import sys

app = Path(sys.argv[1]).resolve()
with (app / "Contents/Info.plist").open("rb") as stream:
    info = plistlib.load(stream)
assert app.suffix == ".app"
assert info["CFBundleIdentifier"] == "com.aimemorandum.app"
assert info["CFBundlePackageType"] == "APPL"
assert info["LSUIElement"] is True
assert info["NSScreenCaptureUsageDescription"]
executable = app / "Contents/MacOS" / info["CFBundleExecutable"]
assert executable.is_file() and os.access(executable, os.X_OK)
assert (app / "Contents/Resources" / info["CFBundleIconFile"]).stat().st_size > 0
subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
identity = json.loads(subprocess.check_output([str(executable), "--app-identity-check"], text=True))
assert identity["bundle_identifier"] == info["CFBundleIdentifier"]
assert identity["bundle_url"] == str(app)
assert identity["is_app_bundle"] is True
libraries = subprocess.check_output(["otool", "-L", str(executable)], text=True)
assert "/.build/" not in "\n".join(libraries.splitlines()[1:]), "Bundle must not depend on build products"
print(json.dumps({"bundle": str(app), "identifier": identity["bundle_identifier"],
    "version": info["CFBundleShortVersionString"], "build": info["CFBundleVersion"],
    "signature": "verified", "runtime_bundle_identity": "verified",
    "privacy_permissions": "controlled by macOS; not modified"}, ensure_ascii=False, indent=2))
