"""Isolated-mode entrypoint; keeps the service package import explicit."""

import os
import sys
from pathlib import Path


def _argument_path(name: str) -> Path | None:
    try:
        index = sys.argv.index(name)
        return Path(sys.argv[index + 1]).resolve()
    except (ValueError, IndexError, OSError):
        return None


_WORK_ROOT = _argument_path("--work-root")
_TESSERACT = _argument_path("--tesseract")


def _inside(path: object, root: Path | None) -> bool:
    if root is None or not isinstance(path, (str, bytes, os.PathLike)):
        return False
    try:
        Path(path).resolve().relative_to(root)
        return True
    except (OSError, ValueError):
        return False


def _enforce_worker_policy(event: str, arguments: tuple[object, ...]) -> None:
    """Deny network, unapproved child processes, and writes outside the job root."""
    if event.startswith("socket.") or event in {
        "http.client.connect",
        "urllib.Request",
    }:
        raise PermissionError("network access is disabled inside the document worker")
    if event == "subprocess.Popen":
        executable = arguments[0] if arguments else None
        try:
            executable_path = Path(str(executable)).resolve()
        except OSError as error:
            raise PermissionError("child process path is invalid") from error
        if _TESSERACT is None or executable_path != _TESSERACT:
            raise PermissionError("only the reviewed OCR engine may be started")
    if event == "open" and len(arguments) >= 2:
        mode = arguments[1]
        if isinstance(mode, str) and any(flag in mode for flag in "wax+"):
            if not _inside(arguments[0], _WORK_ROOT):
                raise PermissionError("worker writes are confined to the job directory")
    if event in {"os.remove", "os.rmdir", "os.mkdir"} and arguments:
        if not _inside(arguments[0], _WORK_ROOT):
            raise PermissionError("worker filesystem changes are confined to the job directory")
    if event in {"os.rename", "os.replace"} and len(arguments) >= 2:
        if not _inside(arguments[0], _WORK_ROOT) or not _inside(arguments[1], _WORK_ROOT):
            raise PermissionError("worker filesystem changes are confined to the job directory")


sys.addaudithook(_enforce_worker_policy)

# Python's isolated mode intentionally omits the script directory. Add only this
# reviewed service directory, never the working directory or user site packages.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from aifos_worker.main import main


if __name__ == "__main__":
    main()
