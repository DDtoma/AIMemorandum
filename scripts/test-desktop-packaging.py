#!/usr/bin/env python3
"""Regression tests for the local desktop package builders (standard library only)."""

from __future__ import annotations

import hashlib
import io
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
LINUX_MANIFEST = ROOT / "apps/desktop/linux/Cargo.toml"


def manifest_version(path: Path) -> str:
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("version"):
            return line.split('"', 2)[1]
    raise AssertionError(f"no package version in {path}")


def host_architecture() -> str:
    machine = platform.machine().lower()
    mapping = {"x86_64": "x86_64", "amd64": "x86_64", "aarch64": "aarch64", "arm64": "aarch64"}
    if machine not in mapping:
        raise unittest.SkipTest(f"unsupported test host architecture: {machine}")
    return mapping[machine]


def run(command: list[str], **kwargs: object) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        **kwargs,
    )


def read_ar(path: Path) -> dict[str, bytes]:
    data = path.read_bytes()
    if not data.startswith(b"!<arch>\n"):
        raise AssertionError(f"{path} is not a Unix ar archive")
    members: dict[str, bytes] = {}
    offset = 8
    while offset < len(data):
        header = data[offset : offset + 60]
        if len(header) != 60 or header[58:60] != b"\x60\n":
            raise AssertionError("invalid ar member header")
        name = header[:16].decode("ascii").strip().rstrip("/")
        size = int(header[48:58].decode("ascii").strip())
        start = offset + 60
        members[name] = data[start : start + size]
        offset = start + size + (size % 2)
    return members


def read_tar_gz(data: bytes) -> dict[str, bytes]:
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        return {
            member.name: archive.extractfile(member).read()
            for member in archive.getmembers()
            if member.isfile()
        }


@unittest.skipUnless(shutil.which("bash") and shutil.which("ar") and shutil.which("tar"), "bash/ar/tar required")
class LinuxPackageTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name)
        self.binary = self.work / "aimemorandum"
        self.binary.write_bytes(b"test executable fixture\n")
        self.binary.chmod(0o755)
        self.dist = self.work / "dist"

    def build(self, *options: str, env: dict[str, str] | None = None) -> None:
        run(
            [
                "bash",
                "scripts/build-linux-packages.sh",
                "--skip-build",
                "--binary",
                self.binary.as_posix(),
                "--output-dir",
                self.dist.as_posix(),
                "--architecture",
                "x86_64",
                *options,
            ],
            env=env,
        )

    def test_deb_contains_versioned_runtime_metadata_and_notices(self) -> None:
        self.build("--deb")
        version = manifest_version(LINUX_MANIFEST)
        package = self.dist / f"aimemorandum_{version}_amd64.deb"
        self.assertTrue(package.is_file())
        archive = read_ar(package)
        self.assertEqual(set(archive), {"debian-binary", "control.tar.gz", "data.tar.gz"})
        self.assertEqual(archive["debian-binary"], b"2.0\n")
        control_files = read_tar_gz(archive["control.tar.gz"])
        control = control_files["./control"].decode("utf-8")
        self.assertIn("Package: aimemorandum\n", control)
        self.assertIn(f"Version: {version}\n", control)
        self.assertIn("Architecture: amd64\n", control)
        data = read_tar_gz(archive["data.tar.gz"])
        expected = {
            "./usr/bin/aimemorandum",
            "./usr/share/applications/aimemorandum.desktop",
            "./usr/share/icons/hicolor/scalable/apps/aimemorandum.svg",
            "./usr/share/doc/aimemorandum/LICENSE",
            "./usr/share/doc/aimemorandum/THIRD_PARTY_NOTICES.md",
        }
        self.assertEqual(set(data), expected)
        self.assertEqual(data["./usr/bin/aimemorandum"], self.binary.read_bytes())
        self.assertEqual(
            data["./usr/share/applications/aimemorandum.desktop"],
            (ROOT / "apps/desktop/linux/packaging/aimemorandum.desktop").read_bytes(),
        )
        self.assertEqual(data["./usr/share/doc/aimemorandum/LICENSE"], (ROOT / "LICENSE").read_bytes())
        self.assertEqual(
            data["./usr/share/doc/aimemorandum/THIRD_PARTY_NOTICES.md"],
            (ROOT / "THIRD_PARTY_NOTICES.md").read_bytes(),
        )

    def test_deb_accepts_relative_output_directory(self) -> None:
        local = ROOT / ".local"
        local.mkdir(exist_ok=True)
        output = Path(tempfile.mkdtemp(prefix="packaging-relative-", dir=local))
        self.addCleanup(shutil.rmtree, output)

        self.build("--deb", "--output-dir", output.relative_to(ROOT).as_posix())

        version = manifest_version(LINUX_MANIFEST)
        self.assertTrue((output / f"aimemorandum_{version}_amd64.deb").is_file())

    def test_builder_rejects_cross_architecture_labels(self) -> None:
        host = host_architecture()
        other = "aarch64" if host == "x86_64" else "x86_64"
        result = subprocess.run(
            [
                "bash",
                "scripts/build-linux-packages.sh",
                "--deb",
                "--skip-build",
                "--binary",
                str(self.binary),
                "--output-dir",
                str(self.dist),
                "--architecture",
                other,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("host architecture", result.stderr)
        self.assertFalse(list(self.dist.glob("*.deb")))

    def test_appimage_stage_and_tool_contract(self) -> None:
        capture = self.work / "captured-appdir"
        arguments = self.work / "appimagetool-arguments"
        architecture = self.work / "appimagetool-architecture"
        tool = self.work / "appimagetool"
        tool.write_text(
            "#!/usr/bin/env bash\n"
            "set -euo pipefail\n"
            "printf '%s\\n' \"$@\" >\"$APPIMAGETOOL_ARGUMENTS\"\n"
            "printf '%s' \"${ARCH:-}\" >\"$APPIMAGETOOL_ARCHITECTURE\"\n"
            "[[ -x \"$1/AppRun\" ]] || { echo 'AppRun is not executable' >&2; exit 1; }\n"
            "cp -R \"$1\" \"$APPIMAGE_CAPTURE\"\n"
            "printf 'AppImage fixture\\n' >\"$2\"\n",
            encoding="utf-8",
        )
        tool.chmod(0o755)
        env = os.environ.copy()
        env.update(
            APPIMAGETOOL=tool.as_posix(),
            APPIMAGE_CAPTURE=capture.as_posix(),
            APPIMAGETOOL_ARGUMENTS=arguments.as_posix(),
            APPIMAGETOOL_ARCHITECTURE=architecture.as_posix(),
        )
        self.build("--appimage", env=env)

        version = manifest_version(LINUX_MANIFEST)
        output = self.dist / f"AIMemorandum-{version}-x86_64.AppImage"
        self.assertEqual(output.read_bytes(), b"AppImage fixture\n")
        args = arguments.read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(args), 2)
        self.assertTrue(args[0].endswith("/AIMemorandum.AppDir"))
        self.assertEqual(Path(args[1]).name, output.name)
        self.assertEqual(architecture.read_text(encoding="utf-8"), "x86_64")

        expected = {
            "AppRun",
            "aimemorandum.desktop",
            "aimemorandum.svg",
            "aimemorandum.png",
            "usr/bin/aimemorandum",
            "usr/share/doc/aimemorandum/LICENSE",
            "usr/share/doc/aimemorandum/THIRD_PARTY_NOTICES.md",
        }
        actual = {str(path.relative_to(capture).as_posix()) for path in capture.rglob("*") if path.is_file()}
        self.assertEqual(actual, expected)
        self.assertEqual((capture / "usr/bin/aimemorandum").read_bytes(), self.binary.read_bytes())
        if os.name != "nt":
            self.assertTrue((capture / "AppRun").stat().st_mode & 0o111)
        app_run = (capture / "AppRun").read_text(encoding="utf-8")
        self.assertIn('exec "$HERE/usr/bin/aimemorandum" "$@"', app_run)
        self.assertNotIn("ExecStart", app_run)
        self.assertEqual(
            (capture / "aimemorandum.desktop").read_bytes(),
            (ROOT / "apps/desktop/linux/packaging/aimemorandum.desktop").read_bytes(),
        )
        self.assertEqual((capture / "aimemorandum.svg").read_bytes(), (ROOT / "apps/desktop/linux/packaging/aimemorandum.svg").read_bytes())
        icon = (capture / "aimemorandum.png").read_bytes()
        self.assertEqual(icon, (ROOT / "apps/desktop/linux/packaging/aimemorandum.png").read_bytes())
        self.assertEqual(icon[:8], b"\x89PNG\r\n\x1a\n")
        self.assertEqual(icon[12:16], b"IHDR")
        self.assertEqual(int.from_bytes(icon[16:20], "big"), 256)
        self.assertEqual(int.from_bytes(icon[20:24], "big"), 256)
        self.assertEqual((capture / "usr/share/doc/aimemorandum/LICENSE").read_bytes(), (ROOT / "LICENSE").read_bytes())
        self.assertEqual(
            (capture / "usr/share/doc/aimemorandum/THIRD_PARTY_NOTICES.md").read_bytes(),
            (ROOT / "THIRD_PARTY_NOTICES.md").read_bytes(),
        )


class NoticeGenerationTests(unittest.TestCase):
    def test_lock_checked_inventory_and_real_license_text_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            source = "registry+https://github.com/rust-lang/crates.io-index"
            lock = work / "Cargo.lock"
            lock_text = (
                "version = 4\n\n"
                "[[package]]\nname = \"alpha\"\nversion = \"1.0.0\"\nsource = \"" + source + "\"\n\n"
                "[[package]]\nname = \"beta\"\nversion = \"2.0.0\"\nsource = \"" + source + "\"\n"
            )
            lock.write_bytes(lock_text.replace("\n", "\r\n").encode("utf-8"))
            alpha = work / "alpha"
            beta = work / "beta"
            alpha.mkdir()
            beta.mkdir()
            (alpha / "Cargo.toml").write_text("[package]\n", encoding="utf-8")
            (beta / "Cargo.toml").write_text("[package]\n", encoding="utf-8")
            (alpha / "LICENSE-MIT").write_bytes(b"MIT fixture license\nCopyright Alpha Author\n")
            metadata = work / "metadata.json"
            metadata.write_text(
                json.dumps(
                    {
                        "packages": [
                            {
                                "name": "alpha",
                                "version": "1.0.0",
                                "source": source,
                                "license": "MIT",
                                "license_file": None,
                                "manifest_path": str(alpha / "Cargo.toml"),
                            },
                            {
                                "name": "beta",
                                "version": "2.0.0",
                                "source": source,
                                "license": "Apache-2.0",
                                "license_file": None,
                                "manifest_path": str(beta / "Cargo.toml"),
                            },
                        ]
                    }
                ),
                encoding="utf-8",
            )
            output = work / "THIRD_PARTY_NOTICES.md"
            command = [
                "python",
                "-B",
                "scripts/generate-third-party-notices.py",
                "--metadata-json",
                str(metadata),
                "--lockfile",
                str(lock),
                "--output",
                str(output),
            ]
            run(command)
            notice = output.read_text(encoding="utf-8")
            self.assertIn(hashlib.sha256(lock_text.encode("utf-8")).hexdigest(), notice)
            self.assertIn("alpha 1.0.0", notice)
            self.assertIn("beta 2.0.0", notice)
            self.assertIn("MIT fixture license\nCopyright Alpha Author", notice)
            self.assertIn("No upstream license/notice text collected", notice)
            self.assertNotIn("Copyright Beta", notice)
            self.assertNotIn(str(work), notice)
            run([*command, "--check"])


class RepositoryContractTests(unittest.TestCase):
    def test_scripts_and_notices_have_no_machine_local_or_placeholder_content(self) -> None:
        sources = [
            ROOT / "scripts/build-linux-packages.sh",
            ROOT / "scripts/build-windows-installer.ps1",
            ROOT / "scripts/generate-third-party-notices.py",
            ROOT / "apps/desktop/windows/packaging/aimemorandum.nsi",
        ]
        forbidden = ["D:\\", "C:\\", ".worktrees", "/home/", "/Users/", "nsis-3.11"]
        for source in sources:
            text = source.read_text(encoding="utf-8")
            for value in forbidden:
                self.assertNotIn(value, text, f"{source}: machine-local path {value}")
        license_text = (ROOT / "LICENSE").read_text(encoding="utf-8")
        self.assertIn("GNU GENERAL PUBLIC LICENSE", license_text)
        self.assertIn("Version 3, 29 June 2007", license_text)
        notices = (ROOT / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
        self.assertIn("@generated by scripts/generate-third-party-notices.py", notices)
        self.assertIn("apps/desktop/Cargo.lock", notices)
        self.assertNotIn("will be replaced", notices)

    def test_unix_packaging_sources_use_lf_endings(self) -> None:
        attributes = (ROOT / ".gitattributes").read_text(encoding="utf-8")
        self.assertIn("*.sh text eol=lf", attributes)
        self.assertIn("*.desktop text eol=lf", attributes)
        self.assertIn("*.py text eol=lf", attributes)
        for relative in [
            "scripts/build-linux-packages.sh",
            "apps/desktop/linux/packaging/aimemorandum.desktop",
            "scripts/test-desktop-packaging.py",
        ]:
            content = (ROOT / relative).read_bytes()
            self.assertNotIn(b"\r\n", content, f"{relative} must stay LF")

    def test_root_and_proposal_docs_describe_desktop_clients(self) -> None:
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        self.assertIn("apps/desktop/", readme)
        self.assertIn("Windows", readme)
        self.assertIn("Linux", readme)
        proposal = (ROOT / "docs/proposals/AM-002-desktop-port/README.md").read_text(encoding="utf-8")
        self.assertIn("已实现", proposal)
        self.assertNotIn("不是已实现的代码", proposal)

    def test_local_packaging_documentation_and_limits(self) -> None:
        run_guide = (ROOT / "docs/RUN.md").read_text(encoding="utf-8")
        for expected in [
            "scripts/build-linux-packages.sh",
            "scripts/build-windows-installer.ps1",
            "scripts/generate-third-party-notices.py",
            "scripts/test-desktop-packaging.py",
            "apps/desktop/dist",
            "APPIMAGETOOL",
            "MAKENSIS",
            "签名",
            "公证",
            "注册",
            "同步",
            "提醒",
            "宿主架构",
            "跨架构",
        ]:
            self.assertIn(expected, run_guide)
        linux = (ROOT / "apps/desktop/linux/README.md").read_text(encoding="utf-8")
        self.assertIn("build-linux-packages.sh", linux)
        self.assertIn("AppImage", linux)
        windows = (ROOT / "apps/desktop/windows/README.md").read_text(encoding="utf-8")
        self.assertIn("build-windows-installer.ps1", windows)
        self.assertIn("/D=", windows)
        self.assertIn("Uninstall.exe", windows)


@unittest.skipUnless(shutil.which("pwsh") or shutil.which("powershell"), "PowerShell required")
class WindowsInstallerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.work = Path(self.temporary.name)
        self.binary = self.work / "aimemorandum-windows.exe"
        self.binary.write_bytes(b"Windows executable fixture\n")
        self.dist = self.work / "dist"
        self.arguments = self.work / "makensis-arguments"
        self.tool = self.work / "makensis-stub.ps1"
        self.tool.write_text(
            "param([Parameter(ValueFromRemainingArguments=$true)][string[]]$Arguments)\n"
            "$Arguments | Set-Content -LiteralPath $env:MAKENSIS_ARGUMENTS -Encoding utf8\n"
            "$define = $Arguments | Where-Object { $_.StartsWith('/DOUTPUT_PATH=') } | Select-Object -First 1\n"
            "if (-not $define) { throw 'missing OUTPUT_PATH define' }\n"
            "$output = $define.Substring('/DOUTPUT_PATH='.Length)\n"
            "[IO.File]::WriteAllText($output, 'NSIS fixture')\n",
            encoding="utf-8",
        )

    def test_builder_defines_and_isolated_uninstall_contract(self) -> None:
        shell = shutil.which("pwsh") or shutil.which("powershell")
        env = os.environ.copy()
        env.update(MAKENSIS=str(self.tool), MAKENSIS_ARGUMENTS=str(self.arguments))
        run(
            [
                shell,
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "scripts/build-windows-installer.ps1",
                "-SkipBuild",
                "-Binary",
                str(self.binary),
                "-OutputDir",
                str(self.dist),
                "-Architecture",
                "x86_64",
            ],
            env=env,
        )
        version = manifest_version(ROOT / "apps/desktop/windows/Cargo.toml")
        output = self.dist / f"AIMemorandum-{version}-x86_64-setup.exe"
        self.assertEqual(output.read_text(encoding="utf-8"), "NSIS fixture")
        args = self.arguments.read_text(encoding="utf-8-sig").splitlines()
        defines = {}
        for argument in args:
            if argument.startswith("/D") and "=" in argument:
                key, value = argument[2:].split("=", 1)
                defines[key] = value
        self.assertEqual(defines["APP_VERSION"], version)
        self.assertEqual(defines["APP_ARCH"], "x86_64")
        self.assertEqual(Path(defines["BINARY_PATH"]), self.binary)
        self.assertEqual(Path(defines["LICENSE_PATH"]), ROOT / "LICENSE")
        self.assertEqual(Path(defines["NOTICES_PATH"]), ROOT / "THIRD_PARTY_NOTICES.md")
        self.assertEqual(Path(defines["OUTPUT_PATH"]), output)
        self.assertEqual(Path(args[-1]), ROOT / "apps/desktop/windows/packaging/aimemorandum.nsi")

        script = (ROOT / "apps/desktop/windows/packaging/aimemorandum.nsi").read_text(encoding="utf-8")
        self.assertIn("RequestExecutionLevel user", script)
        self.assertIn("SetShellVarContext current", script)
        self.assertNotIn("WriteReg", script)
        self.assertNotIn("DeleteReg", script)
        self.assertNotIn("CreateShortcut", script)
        self.assertNotIn("MUI_FINISHPAGE_RUN", script)
        self.assertNotRegex(script, r"(?im)^\s*Exec(?:Shell)?\s")

    def test_builder_rejects_cross_architecture_labels(self) -> None:
        shell = shutil.which("pwsh") or shutil.which("powershell")
        host = host_architecture()
        other = "aarch64" if host == "x86_64" else "x86_64"
        env = os.environ.copy()
        env.update(MAKENSIS=str(self.tool), MAKENSIS_ARGUMENTS=str(self.arguments))
        result = subprocess.run(
            [
                shell,
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "scripts/build-windows-installer.ps1",
                "-SkipBuild",
                "-Binary",
                str(self.binary),
                "-OutputDir",
                str(self.dist),
                "-Architecture",
                other,
            ],
            cwd=ROOT,
            env=env,
            capture_output=True,
            text=True,
            errors="replace",
            check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("host architecture", result.stdout + result.stderr)
        self.assertFalse(list(self.dist.glob("*.exe")))


if __name__ == "__main__":
    unittest.main(verbosity=2)
