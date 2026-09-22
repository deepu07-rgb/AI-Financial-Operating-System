"""Deterministic local rasterization and invoice-image preprocessing."""

from __future__ import annotations

import hashlib
import io
from dataclasses import dataclass
from pathlib import Path
from collections.abc import Generator, Iterable

import pypdfium2 as pdfium
from PIL import Image, ImageFilter, ImageOps

from . import PIPELINE_VERSION

MAX_PAGES = 20
MAX_IMAGE_PIXELS = 50_000_000
PDF_DPI = 200
Image.MAX_IMAGE_PIXELS = MAX_IMAGE_PIXELS


class PreprocessingError(RuntimeError):
    """Raised for unsafe or unsupported source rasterization."""


@dataclass(frozen=True)
class ProcessedPage:
    page_index: int
    path: Path
    width: int
    height: int
    sha256: str
    deskew_degrees: int


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(64 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def preprocess_document(
    source: Path, media_type: str, output_directory: Path
) -> list[ProcessedPage]:
    output_directory.mkdir(parents=True, exist_ok=False)
    images = _rasterize(source, media_type)
    pages: list[ProcessedPage] = []
    try:
        for page_index, image in enumerate(images):
            try:
                processed, angle = _preprocess_image(image)
                try:
                    if processed.width * processed.height > MAX_IMAGE_PIXELS:
                        raise PreprocessingError("processed page exceeds the pixel limit")
                    destination = output_directory / f"page-{page_index + 1:04d}.png"
                    processed.save(destination, "PNG", compress_level=6, optimize=False)
                    pages.append(
                        ProcessedPage(
                            page_index=page_index,
                            path=destination,
                            width=processed.width,
                            height=processed.height,
                            sha256=sha256_file(destination),
                            deskew_degrees=angle,
                        )
                    )
                finally:
                    processed.close()
            finally:
                image.close()
    finally:
        images.close()
    if not pages:
        raise PreprocessingError("document has no pages")
    return pages


def preprocessing_manifest(pages: Iterable[ProcessedPage]) -> dict[str, object]:
    return {
        "pipeline_version": PIPELINE_VERSION,
        "steps": [
            "orientation",
            "grayscale",
            "deskew",
            "denoise",
            "autocontrast",
            "content_crop",
            "page_segmentation",
        ],
        "derivatives": [
            {
                "page_index": page.page_index,
                "width": page.width,
                "height": page.height,
                "sha256": page.sha256,
                "deskew_degrees": page.deskew_degrees,
            }
            for page in pages
        ],
    }


def render_page_preview(
    source: Path, media_type: str, requested_page_index: int
) -> tuple[int, int, bytes]:
    images = _rasterize(source, media_type)
    try:
        for page_index, image in enumerate(images):
            try:
                if page_index != requested_page_index:
                    continue
                processed, _ = _preprocess_image(image)
                try:
                    width, height = processed.size
                    preview = processed.convert("RGB")
                    try:
                        preview.thumbnail((1600, 2000), Image.Resampling.LANCZOS)
                        output = io.BytesIO()
                        preview.save(
                            output,
                            "JPEG",
                            quality=85,
                            optimize=False,
                            progressive=False,
                        )
                        return width, height, output.getvalue()
                    finally:
                        preview.close()
                finally:
                    processed.close()
            finally:
                image.close()
    finally:
        images.close()
    raise PreprocessingError("requested page is outside the document page range")


def _rasterize(source: Path, media_type: str) -> Generator[Image.Image, None, None]:
    if media_type == "application/pdf":
        document = pdfium.PdfDocument(source)
        try:
            page_count = len(document)
            if page_count == 0 or page_count > MAX_PAGES:
                raise PreprocessingError(f"PDF page count must be between 1 and {MAX_PAGES}")
            for page_index in range(page_count):
                page = document[page_index]
                try:
                    bitmap = page.render(scale=PDF_DPI / 72)
                    try:
                        image = bitmap.to_pil().convert("RGB")
                    finally:
                        bitmap.close()
                finally:
                    page.close()
                if image.width * image.height > MAX_IMAGE_PIXELS:
                    image.close()
                    raise PreprocessingError("rasterized page exceeds the pixel limit")
                yield image
            return
        finally:
            document.close()
    try:
        with Image.open(source) as opened:
            opened.load()
            oriented = ImageOps.exif_transpose(opened)
            yield oriented.convert("RGB")
    except (Image.DecompressionBombError, OSError) as error:
        raise PreprocessingError("image cannot be decoded safely") from error


def _preprocess_image(image: Image.Image) -> tuple[Image.Image, int]:
    grayscale = ImageOps.grayscale(image)
    try:
        angle = _estimate_deskew(grayscale)
        if angle:
            rotated = grayscale.rotate(
                angle,
                resample=Image.Resampling.BICUBIC,
                expand=True,
                fillcolor=255,
            )
            grayscale.close()
            grayscale = rotated
        denoised = grayscale.filter(ImageFilter.MedianFilter(size=3))
        try:
            contrasted = ImageOps.autocontrast(denoised, cutoff=1)
            try:
                return _crop_content(contrasted), angle
            finally:
                contrasted.close()
        finally:
            denoised.close()
    finally:
        grayscale.close()


def _estimate_deskew(image: Image.Image) -> int:
    sample = image.copy()
    sample.thumbnail((900, 900), Image.Resampling.BILINEAR)
    best_angle = 0
    best_score = -1
    for angle in range(-3, 4):
        candidate = sample.rotate(
            angle,
            resample=Image.Resampling.BILINEAR,
            expand=True,
            fillcolor=255,
        )
        thresholded = candidate.point(lambda value: 1 if value < 180 else 0)
        histogram = [sum(row) for row in _rows(thresholded)]
        score = sum(
            (current - previous) * (current - previous)
            for previous, current in zip(histogram, histogram[1:])
        )
        candidate.close()
        thresholded.close()
        if score > best_score:
            best_score = score
            best_angle = angle
    sample.close()
    return best_angle


def _rows(image: Image.Image) -> Iterable[list[int]]:
    # Pillow 14 renamed getdata() to get_flattened_data(). The signed worker
    # supports both the bundled runtime and older pilot runtimes deterministically.
    flattened = getattr(image, "get_flattened_data", None)
    pixels = list(flattened() if flattened is not None else image.getdata())
    width = image.width
    for offset in range(0, len(pixels), width):
        yield pixels[offset : offset + width]


def _crop_content(image: Image.Image) -> Image.Image:
    ink = image.point(lambda value: 255 if value < 245 else 0)
    box = ink.getbbox()
    ink.close()
    if box is None:
        return image.copy()
    margin = 12
    left = max(0, box[0] - margin)
    top = max(0, box[1] - margin)
    right = min(image.width, box[2] + margin)
    bottom = min(image.height, box[3] + margin)
    return image.crop((left, top, right, bottom))
