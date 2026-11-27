"""Build a native Scrapburn distribution. Run from the repository root.

Usage: package.py [RELEASE_DIR] [--windows]

RELEASE_DIR defaults to target/release. --windows packages a game binary that was
cross-compiled for x86_64 Windows, building the helper for Windows to match.
"""

import os
import platform
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


def main() -> None:
    root = Path.cwd()
    args = [a for a in sys.argv[1:] if a != "--windows"]
    cross = "--windows" in sys.argv[1:]
    windows = cross or os.name == "nt"
    release = root / (args[0] if args else "target/release")
    dist = root / "target" / "dist"
    helper = root / "tools" / "tailcat"
    dist.mkdir(parents=True, exist_ok=True)
    suffix = ".exe" if windows else ""
    system = "windows" if cross else platform.system().lower()
    machine = "x86_64" if cross else platform.machine().lower()
    machine = {"amd64": "x86_64", "arm64": "aarch64"}.get(machine, machine)
    # A pure-Go static binary has no glibc dependency at all.
    go_env = {**os.environ, "CGO_ENABLED": "0"}
    if cross:
        go_env |= {"GOOS": "windows", "GOARCH": "amd64"}
    version = subprocess.check_output(
        ["git", "describe", "--tags", "--always", "--dirty"], text=True
    ).strip()
    name = f"scrapburn-{version}-{system}-{machine}"
    with tempfile.TemporaryDirectory(dir=dist) as temporary:
        stage = Path(temporary) / name
        stage.mkdir()
        game = stage / f"scrapburn{suffix}"
        shutil.copy2(release / game.name, game)
        # The Windows cross build is already stripped by cargo.
        if not windows and shutil.which("strip"):
            subprocess.run(["strip", game], check=True)
        go = ["mise", "exec", "--", "go"]
        subprocess.run(
            [
                *go,
                "build",
                "-trimpath",
                "-ldflags=-s -w",
                "-o",
                stage / f"scrapburn-tailcat{suffix}",
                ".",
            ],
            cwd=helper,
            check=True,
            env=go_env,
        )
        assets = subprocess.check_output(
            [
                "git",
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
                "assets/textures",
                "assets/fonts",
                "assets/models",
                ":!assets/tripo-spend.tsv",
            ]
        )
        for entry in assets.split(b"\0"):
            if entry:
                path = Path(os.fsdecode(entry))
                (stage / path.parent).mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, stage / path)
        for path in ("LICENSE-APACHE", "LICENSE-MIT", "README.md"):
            shutil.copy2(path, stage / path)
        # Preserve notices from the Go runtime and the modules linked into the helper.
        modules = subprocess.check_output(
            [
                *go,
                "list",
                "-deps",
                "-f",
                "{{if .Module}}{{.Module.Path}}|{{.Module.Dir}}{{end}}",
                ".",
            ],
            cwd=helper,
            text=True,
            env=go_env,
        )
        goroot = subprocess.check_output(
            [*go, "env", "GOROOT"], cwd=helper, text=True
        ).strip()
        for item in sorted(set(modules.splitlines()) | {f"Go|{goroot}"}):
            if not item:
                continue
            module, directory = item.split("|", 1)
            for file in Path(directory).iterdir():
                if file.is_file() and file.name.upper().startswith(
                    ("LICENSE", "COPYING", "NOTICE", "PATENTS")
                ):
                    target = stage / "licenses" / module / file.name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(file, target)
        archive = Path(
            shutil.make_archive(
                str(Path(temporary) / name),
                "zip" if windows else "gztar",
                temporary,
                name,
            )
        )
        destination = dist / archive.name
        archive.replace(destination)
        print(destination)


if __name__ == "__main__":
    main()
