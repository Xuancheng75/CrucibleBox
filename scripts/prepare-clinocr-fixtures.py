#!/usr/bin/env python3
"""Create raster-only PDF fixtures from ClinOCR-Bench v1.0 scans and audited text."""

import argparse
import hashlib
import io
import json
import zipfile
from pathlib import Path

from PIL import Image


SAMPLES = (
    ("normal", "template_1_sample_2_normal"),
    ("poor", "template_1_sample_2_poor"),
    ("tables", "template_9_sample_2_tables"),
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    manifest = {
        "source": "https://github.com/ClinOCR-Bench/ClinOCR-Bench/releases/tag/v1.0",
        "archive_sha256": sha256(args.archive.read_bytes()),
        "license": "MIT",
        "pdf_method": "Source JPEG decoded to RGB and saved as the sole raster PDF page; no text layer",
        "samples": [],
    }
    with zipfile.ZipFile(args.archive) as bundle:
        for subset, sample in SAMPLES:
            scan_name = f"ClinOCR-Bench/scans/{subset}/{sample}.jpg"
            truth_name = f"ClinOCR-Bench/ground_truth/{subset}/{sample}.txt"
            scan = bundle.read(scan_name)
            truth = bundle.read(truth_name)
            if not truth.decode("utf-8-sig").strip():
                raise ValueError(f"empty ground truth: {truth_name}")
            pdf_path = args.output_dir / f"{sample}.pdf"
            truth_path = args.output_dir / f"{sample}.txt"
            with Image.open(io.BytesIO(scan)) as image:
                image.convert("RGB").save(pdf_path, "PDF", resolution=200)
            truth_path.write_bytes(truth)
            manifest["samples"].append(
                {
                    "subset": subset,
                    "source_scan": scan_name,
                    "scan_sha256": sha256(scan),
                    "source_truth": truth_name,
                    "truth_sha256": sha256(truth),
                    "pdf": pdf_path.name,
                    "pdf_sha256": sha256(pdf_path.read_bytes()),
                }
            )
    manifest_path = args.output_dir / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2), encoding="utf-8")
    print(manifest_path)


if __name__ == "__main__":
    main()
