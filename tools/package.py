"""Build a native Scrapburn distribution. Run from the repository root."""

import os
import platform
import shutil
import subprocess
import tempfile
from pathlib import Path


def main() -> None:
    root = Path.cwd()
    release = root / "target" / "release"
    dist = root / "target" / "dist"
    helper = root / "tools" / "tailcat"
    dist.mkdir(parents=True, exist_ok=True)
    suffix = ".exe" if os.name == "nt" else ""
    version = subprocess.check_output(
        ["git", "describe", "--tags", "--always", "--dirty"], text=True
    ).strip()
    name = (
        f"scrapburn-{version}-{platform.system().lower()}-{platform.machine().lower()}"
    )
    with tempfile.TemporaryDirectory(dir=dist) as temporary:
        stage = Path(temporary) / name
        stage.mkdir()
        game = stage / f"scrapburn{suffix}"
        shutil.copy2(release / game.name, game)
        if os.name != "nt" and shutil.which("strip"):
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
                "zip" if os.name == "nt" else "gztar",
                temporary,
                name,
            )
        )
        destination = dist / archive.name
        archive.replace(destination)
        print(destination)


if __name__ == "__main__":
    main()
