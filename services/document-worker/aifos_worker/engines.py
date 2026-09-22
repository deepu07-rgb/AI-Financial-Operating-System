"""Replaceable OCR engine contract and a bounded Tesseract CLI adapter."""

from __future__ import annotations

import csv
import io
import os
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Protocol

MAX_ENGINE_OUTPUT_BYTES = 8 * 1024 * 1024
ENGINE_TIMEOUT_SECONDS = 30


class EngineError(RuntimeError):
    """Raised when an OCR engine is missing or violates its adapter contract."""


@dataclass(frozen=True)
class OcrWord:
    page_index: int
    text: str
    confidence: float
    left: int
    top: int
    width: int
    height: int
    block: int
    paragraph: int
    line: int


class OcrEngine(Protocol):
    @property
    def identity(self) -> dict[str, str]: ...

    def recognize(self, image_path: Path, page_index: int) -> list[OcrWord]: ...


class TesseractCliEngine:
    def __init__(self, executable: Path, language: str = "eng") -> None:
        self._executable = executable.resolve(strict=True)
        self._language = language
        version = _run_bounded([str(self._executable), "--version"], 5).splitlines()
        if not version or not version[0].lower().startswith("tesseract "):
            raise EngineError("configured OCR executable is not Tesseract")
        self._version = version[0].split(maxsplit=1)[1].strip()

    @property
    def identity(self) -> dict[str, str]:
        return {
            "name": "tesseract-cli",
            "version": self._version,
            "language": self._language,
        }

    def recognize(self, image_path: Path, page_index: int) -> list[OcrWord]:
        output = _run_bounded(
            [
                str(self._executable),
                str(image_path),
                "stdout",
                "-l",
                self._language,
                "--oem",
                "1",
                "--psm",
                "6",
                "tsv",
            ],
            ENGINE_TIMEOUT_SECONDS,
        )
        words = []
        for row in csv.DictReader(io.StringIO(output), delimiter="\t"):
            text = (row.get("text") or "").strip()
            try:
                confidence = float(row.get("conf", "-1"))
                level = int(row.get("level", "0"))
            except ValueError as error:
                raise EngineError("Tesseract returned malformed TSV") from error
            if level != 5 or not text or confidence < 0:
                continue
            try:
                words.append(
                    OcrWord(
                        page_index=page_index,
                        text=text,
                        confidence=round(min(confidence, 100.0) / 100.0, 4),
                        left=int(row["left"]),
                        top=int(row["top"]),
                        width=int(row["width"]),
                        height=int(row["height"]),
                        block=int(row["block_num"]),
                        paragraph=int(row["par_num"]),
                        line=int(row["line_num"]),
                    )
                )
            except (KeyError, ValueError) as error:
                raise EngineError("Tesseract returned malformed TSV") from error
        return words


def _run_bounded(arguments: list[str], timeout_seconds: int) -> str:
    environment = {
        "PATH": os.environ.get("PATH", ""),
        "SYSTEMROOT": os.environ.get("SYSTEMROOT", ""),
        "TESSDATA_PREFIX": os.environ.get("TESSDATA_PREFIX", ""),
    }
    try:
        completed = subprocess.run(
            arguments,
            executable=arguments[0],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
            env=environment,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise EngineError(
            f"OCR engine could not complete within its process boundary: {error}"
        ) from error
    if len(completed.stdout) > MAX_ENGINE_OUTPUT_BYTES:
        raise EngineError("OCR engine output exceeded the permitted size")
    if completed.returncode != 0:
        message = completed.stderr[:2048].decode("utf-8", errors="replace").strip()
        raise EngineError(f"OCR engine failed: {message or 'no diagnostic'}")
    return completed.stdout.decode("utf-8", errors="strict")
