#!/usr/bin/env python3
"""Score one OCR output against an independent, human-audited transcript."""

import argparse
import json
import re
import unicodedata
from pathlib import Path


def normalize(text: str) -> str:
    return re.sub(r"\s+", " ", unicodedata.normalize("NFKC", text).casefold()).strip()


def edit_distance(reference, prediction) -> int:
    previous = list(range(len(prediction) + 1))
    for row, item in enumerate(reference, 1):
        current = [row]
        for col, other in enumerate(prediction, 1):
            current.append(
                min(
                    previous[col] + 1,
                    current[col - 1] + 1,
                    previous[col - 1] + (item != other),
                )
            )
        previous = current
    return previous[-1]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ground_truth", type=Path)
    parser.add_argument("ocr_output", type=Path)
    args = parser.parse_args()
    reference = normalize(args.ground_truth.read_text(encoding="utf-8-sig"))
    prediction = normalize(args.ocr_output.read_text(encoding="utf-8-sig"))
    if not reference:
        parser.error("ground truth is empty")
    ref_words = reference.split()
    pred_words = prediction.split()
    char_edits = edit_distance(reference, prediction)
    word_edits = edit_distance(ref_words, pred_words)
    print(
        json.dumps(
            {
                "ground_truth": str(args.ground_truth),
                "ocr_output": str(args.ocr_output),
                "normalization": "Unicode NFKC, casefold, collapse whitespace",
                "reference_characters": len(reference),
                "reference_words": len(ref_words),
                "character_edits": char_edits,
                "word_edits": word_edits,
                "cer": round(char_edits / len(reference), 6),
                "wer": round(word_edits / len(ref_words), 6) if ref_words else None,
            },
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
