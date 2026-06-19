#!/usr/bin/env python3
"""
bootstrap.py - unified build/test/check/install for this Rust project.

Self-configuring: reads `cargo metadata` to discover bin targets, PyO3/maturin
modules (cdylib crates that depend on pyo3) and an optional `xtask` helper.
Release by default. Cross-platform, Python 3 stdlib only.

Commands:
    b(uild)   [py]    Build the workspace (release). `b py` -> maturin-build the PyO3 module(s).
    t(est)    [py]    cargo test --workspace. `t py` -> maturin develop + pytest.
    c(heck)           cargo fmt --check + clippy --all-targets -D warnings.
    i(nstall) [py]    Install CLI bin(s) (cargo install) AND the PyO3 module(s) (maturin).
                      `i py` -> module(s) only.
    m(odule)          maturin develop --release: build + install the PyO3 module into
                      the active Python (importable immediately).
    x  <args...>      Passthrough to `cargo xtask <args>` (only if an `xtask` crate exists).
    cl(ean)           cargo clean.
    h(elp)            Show this help.

Flags:
    -d, --debug       Debug profile (default: release).
    -v, --verbose     Stream child output.

Examples:
    python bootstrap.py b            # build everything (release)
    python bootstrap.py b py         # build the PyO3 wheel(s) via maturin
    python bootstrap.py t            # cargo test --workspace
    python bootstrap.py m            # maturin develop (install module into current venv)
    python bootstrap.py i            # install CLI bin(s) + PyO3 module(s)
    python bootstrap.py c            # fmt --check + clippy
    python bootstrap.py x build      # cargo xtask build (projects with an xtask)
"""
from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).parent.resolve()


class C:
    RST = "\033[0m"; RED = "\033[91m"; GRN = "\033[92m"; YLW = "\033[93m"; CYN = "\033[96m"; WHT = "\033[97m"

    @classmethod
    def init(cls) -> None:
        if platform.system() == "Windows":
            os.system("")


def header(t: str) -> None:
    ln = "=" * 60
    print(f"\n{C.CYN}{ln}\n{t}\n{ln}{C.RST}")


def step(t: str) -> None: print(f"  {C.WHT}{t}{C.RST}")
def ok(t: str) -> None: print(f"  {C.GRN}{t}{C.RST}")
def err(t: str) -> None: print(f"  {C.RED}{t}{C.RST}")
def warn(t: str) -> None: print(f"  {C.YLW}{t}{C.RST}")


def fmt_time(ms: float) -> str:
    if ms < 1000: return f"{ms:.0f}ms"
    if ms < 60_000: return f"{ms / 1000:.1f}s"
    return f"{int(ms // 60_000)}m{(ms % 60_000) / 1000:.0f}s"


def run(args: list[str], capture: bool = False) -> tuple[int, str, float]:
    t0 = time.perf_counter()
    r = subprocess.run(args, cwd=ROOT, capture_output=capture, text=True, encoding="utf-8", errors="replace")
    ms = (time.perf_counter() - t0) * 1000
    return r.returncode, ((r.stdout or "") + (r.stderr or "") if capture else ""), ms


def has(tool: str) -> bool:
    return shutil.which(tool) is not None


# ---- self-configuration via cargo metadata -------------------------------
_META: dict | None = None


def meta() -> dict:
    global _META
    if _META is None:
        try:
            r = subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"],
                               cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace")
            _META = json.loads(r.stdout) if r.returncode == 0 else {}
        except Exception:
            _META = {}
    return _META


def local_pkgs() -> list[dict]:
    m = meta()
    ids = set(m.get("workspace_members", []))
    return [p for p in m.get("packages", []) if p["id"] in ids] or m.get("packages", [])


def pyo3_manifests() -> list[Path]:
    """cdylib crates that depend on pyo3 -> maturin targets."""
    out = []
    for p in local_pkgs():
        kinds = {k for t in p.get("targets", []) for k in t.get("crate_types", [])}
        deps = {d["name"] for d in p.get("dependencies", [])}
        if "cdylib" in kinds and "pyo3" in deps:
            out.append(Path(p["manifest_path"]))
    return out


def bin_packages() -> list[str]:
    return sorted({p["name"] for p in local_pkgs()
                   for t in p.get("targets", []) if "bin" in t.get("kind", [])})


def has_xtask() -> bool:
    return any(p["name"] == "xtask" for p in meta().get("packages", []))


def rel_flag(debug: bool) -> list[str]:
    return [] if debug else ["--release"]


# ---- commands ------------------------------------------------------------
def build(debug: bool, py: bool) -> int:
    if py:
        return build_py(debug)
    header("BUILD (workspace)")
    step(f"Mode: {'debug' if debug else 'release'}")
    code, _, ms = run(["cargo", "build", "--workspace"] + rel_flag(debug))
    (ok if code == 0 else err)(f"build {'OK' if code == 0 else 'FAILED'} ({fmt_time(ms)})")
    return code


def build_py(debug: bool) -> int:
    header("BUILD PyO3 MODULE(S) (maturin)")
    mans = pyo3_manifests()
    if not mans:
        warn("no PyO3 (cdylib + pyo3) crate found in this workspace.")
        return 0
    if not has("maturin"):
        err("maturin not found -> pip install maturin (or pipx install maturin)")
        return 1
    rc = 0
    for man in mans:
        step(f"maturin build -m {man.relative_to(ROOT)}")
        code, out, ms = run(["maturin", "build", "-m", str(man)] + rel_flag(debug), capture=True)
        if code == 0:
            ok(f"{man.parent.name} OK ({fmt_time(ms)})")
        else:
            err(f"{man.parent.name} FAILED")
            for line in out.strip().splitlines()[-10:]:
                step(line)
            rc = code
    return rc


def module(debug: bool) -> int:
    """maturin develop: build + install the module into the active python."""
    header("INSTALL PyO3 MODULE (maturin develop)")
    mans = pyo3_manifests()
    if not mans:
        warn("no PyO3 crate found.")
        return 0
    if not has("maturin"):
        err("maturin not found -> pip install maturin")
        return 1
    rc = 0
    for man in mans:
        step(f"maturin develop -m {man.relative_to(ROOT)}")
        code, _, ms = run(["maturin", "develop", "-m", str(man)] + rel_flag(debug))
        (ok if code == 0 else err)(f"{man.parent.name} {'installed' if code == 0 else 'FAILED'} ({fmt_time(ms)})")
        rc = rc or code
    return rc


def test(debug: bool, py: bool) -> int:
    header("TEST")
    if py:
        rc = module(debug)
        if rc:
            return rc
        if not has("pytest") and subprocess.run([sys.executable, "-m", "pytest", "--version"],
                                                capture_output=True).returncode != 0:
            warn("pytest not found -> pip install pytest")
            return 1
        code, _, ms = run([sys.executable, "-m", "pytest"])
        (ok if code == 0 else err)(f"pytest {'OK' if code == 0 else 'FAILED'} ({fmt_time(ms)})")
        return code
    code, _, ms = run(["cargo", "test", "--workspace"] + rel_flag(debug))
    (ok if code == 0 else err)(f"tests {'OK' if code == 0 else 'FAILED'} ({fmt_time(ms)})")
    return code


def check() -> int:
    header("CHECK (fmt + clippy)")
    code, out, _ = run(["cargo", "fmt", "--check"], capture=True)
    if code != 0:
        err("fmt check failed (run `cargo fmt`)")
        for line in out.strip().splitlines()[:10]:
            step(line)
        return code
    ok("fmt OK")
    code, _, ms = run(["cargo", "clippy", "--all-targets", "--", "-D", "warnings"])
    (ok if code == 0 else err)(f"clippy {'OK' if code == 0 else 'FAILED'} ({fmt_time(ms)})")
    return code


def install(debug: bool, py: bool) -> int:
    header("INSTALL")
    rc = 0
    if not py:
        bins = bin_packages()
        if bins:
            for b in bins:
                step(f"cargo install --path . --bin {b}")
                code, _, ms = run(["cargo", "install", "--path", ".", "--bin", b, "--force"]
                                  + ([] if debug else []))
                (ok if code == 0 else err)(f"{b} {'installed' if code == 0 else 'FAILED'} ({fmt_time(ms)})")
                rc = rc or code
        else:
            warn("no bin targets to install.")
    if pyo3_manifests():
        rc = rc or module(debug)
    return rc


def clean() -> int:
    header("CLEAN")
    code, _, ms = run(["cargo", "clean"])
    (ok if code == 0 else err)(f"clean {'OK' if code == 0 else 'FAILED'} ({fmt_time(ms)})")
    return code


def xtask(rest: list[str]) -> int:
    header("XTASK")
    if not has_xtask():
        err("no `xtask` crate in this workspace.")
        return 1
    code, _, _ = run(["cargo", "xtask"] + rest)
    return code


def main() -> int:
    C.init()
    argv = sys.argv[1:]
    debug = False
    rest = []
    for a in argv:
        if a in ("-d", "--debug"):
            debug = True
        elif a in ("-v", "--verbose"):
            pass
        else:
            rest.append(a)
    cmd = rest[0] if rest else "help"
    sub = rest[1] if len(rest) > 1 else None
    py = sub in ("py", "p", "python")
    if cmd in ("b", "build"): return build(debug, py)
    if cmd in ("t", "test"): return test(debug, py)
    if cmd in ("c", "ch", "check"): return check()
    if cmd in ("i", "install"): return install(debug, py)
    if cmd in ("m", "module"): return module(debug)
    if cmd in ("x", "xtask"): return xtask(rest[1:])
    if cmd in ("cl", "clean"): return clean()
    print(__doc__)
    return 0


if __name__ == "__main__":
    sys.exit(main())
