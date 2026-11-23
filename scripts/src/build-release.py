"""Build and package SYS-Radar for Windows/Linux x86, x86_64, and ARM64."""

import argparse
import hashlib
import os
import shutil
import subprocess
import tarfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TARGETS = {
    "x86_64-unknown-linux-musl": ("linux", "x86_64"),
    "i686-unknown-linux-musl": ("linux", "x86"),
    "aarch64-unknown-linux-musl": ("linux", "arm64"),
    "x86_64-pc-windows-gnu": ("windows", "x86_64"),
    "i686-pc-windows-gnu": ("windows", "x86"),
    "aarch64-pc-windows-gnullvm": ("windows", "arm64"),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--target",
        action="append",
        choices=TARGETS,
        help="Build only this target; repeat for several targets (default: all targets)",
    )
    parser.add_argument(
        "--llvm-mingw",
        type=Path,
        help="LLVM-MinGW directory (needed for Windows ARM64)",
    )
    args = parser.parse_args()
    targets = args.target if args.target is not None else list(TARGETS)
    manifest = ROOT / "Cargo.toml"
    if not manifest.is_file():
        parser.error(f"Cargo.toml not found at {manifest}")
    version = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]["version"]
    output = ROOT / "release"
    binaries = output / "bin"
    binaries.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    if args.llvm_mingw:
        compiler = args.llvm_mingw.resolve() / "bin" / "aarch64-w64-mingw32-clang"
        if not compiler.is_file():
            parser.error(f"LLVM-MinGW compiler not found: {compiler}")
        environment["CARGO_TARGET_AARCH64_PC_WINDOWS_GNULLVM_LINKER"] = str(compiler)
    elif compiler := shutil.which("aarch64-w64-mingw32-clang"):
        environment["CARGO_TARGET_AARCH64_PC_WINDOWS_GNULLVM_LINKER"] = compiler

    if "aarch64-unknown-linux-musl" in targets and not environment.get(
        "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER"
    ):
        linker = shutil.which("aarch64-linux-musl-gcc")
        if linker:
            environment["CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER"] = linker
    # Keep build artifacts at a predictable location even with CARGO_TARGET_DIR set.
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target")

    archives = []
    failures = []
    for target in targets:
        system, arch = TARGETS[target]
        if target == "aarch64-unknown-linux-musl" and not environment.get(
            "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER"
        ):
            message = (
                f"Skipping {target}: install an aarch64 musl cross linker "
                "(aarch64-linux-musl-gcc) or set "
                "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER."
            )
            print(message, flush=True)
            failures.append(f"{target}: missing linker")
            continue
        if target == "aarch64-pc-windows-gnullvm" and not environment.get(
            "CARGO_TARGET_AARCH64_PC_WINDOWS_GNULLVM_LINKER"
        ):
            message = (
                f"Skipping {target}: pass --llvm-mingw <directory> or put "
                "aarch64-w64-mingw32-clang on PATH."
            )
            print(message, flush=True)
            failures.append(f"{target}: missing LLVM-MinGW")
            continue
        result = subprocess.run(
            [
                "cargo",
                "build",
                "--locked",
                "--offline",
                "--release",
                "--bins",
                "--target",
                target,
            ],
            cwd=ROOT,
            env=environment,
            check=False,
        )
        if result.returncode:
            failures.append(f"{target}: cargo build exited with status {result.returncode}")
            print(f"Build failed for {target}; continuing with remaining targets.", flush=True)
            continue
        name = f"SYS-Radar-v{version}-{system}-{arch}"
        extension = ".exe" if system == "windows" else ""
        executable = binaries / f"{name}{extension}"
        shutil.copy2(
            ROOT / "target" / target / "release" / f"sys-radar{extension}",
            executable,
        )
        if system == "windows":
            archive = output / f"{name}.zip"
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as package:
                package.write(executable, executable.name)
        else:
            archive = output / f"{name}.tar.gz"
            with tarfile.open(archive, "w:gz") as package:
                package.add(executable, arcname=executable.name)
        archives.append(archive)
        print(f"Packaged {archive.name}", flush=True)

    checksums = "".join(
        f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n"
        for archive in sorted(archives)
    )
    (output / "SHA256SUMS.txt").write_text(checksums, encoding="utf-8")
    print(f"Release files: {output}")
    if failures:
        print("Some requested targets were not built:", flush=True)
        for failure in failures:
            print(f"  - {failure}", flush=True)
        raise SystemExit(1)


if __name__ == "__main__":
    main()
