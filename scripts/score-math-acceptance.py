"""Score display-formula transcription in paired OHR-Bench PDF/GT fixtures.

The expected local names are sample-N.pdf, sample-N.json and sample-N.ir.json.
Candidate counts are only an upper bound; this also reports exact LaTeX matches.
"""

import argparse
import hashlib
import json
import re
from difflib import SequenceMatcher
from pathlib import Path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def normalize_latex(value: str) -> str:
    return re.sub(r"\s+", "", value)


def diagnostic_tokens(value: str) -> list[str]:
    """Ignore layout syntax for candidate pairing; never use this as a pass metric."""
    value = re.sub(r"\\begin\{(?:array|aligned|align\*?)\}(?:\{[^}]*\})?", "", value)
    value = re.sub(r"\\end\{(?:array|aligned|align\*?)\}", "", value)
    value = re.sub(
        r"\\(?:displaystyle|textstyle|quad|qquad|thinspace|left|right|big|Big|bigg|Bigg)\b",
        "",
        value,
    )
    value = re.sub(r"\\[,;! ]", "", value)
    value = re.sub(r"\s+", "", value)
    value = re.sub(r"[{}&]", "", value)
    return re.findall(r"\\[A-Za-z]+|\\.|[A-Za-z0-9]+|[^\s]", value)


def score(root: Path, sample: str, ir_suffix: str) -> dict:
    pdf = root / f"{sample}.pdf"
    ground_truth = root / f"{sample}.json"
    result = root / f"{sample}{ir_suffix}"
    text = json.loads(ground_truth.read_text(encoding="utf-8"))[0]["text"]
    display_formulas = re.findall(r"\$\$(.*?)\$\$", text, re.DOTALL)
    pages = json.loads(result.read_text(encoding="utf-8"))["document"]["pages"]
    recognized = [
        block
        for page in pages
        for block in page["blocks"]
        if block["type"] in {"formula", "matrix"}
    ]
    candidates = [normalize_latex(str(block.get("content", ""))) for block in recognized]
    comparisons = []
    for expected in display_formulas:
        target = normalize_latex(expected)
        best = max(
            candidates,
            key=lambda candidate: SequenceMatcher(None, target, candidate).ratio(),
            default="",
        )
        structural_target = diagnostic_tokens(expected)
        best_structural = max(
            candidates,
            key=lambda candidate: SequenceMatcher(
                None, structural_target, diagnostic_tokens(candidate)
            ).ratio(),
            default="",
        )
        comparisons.append(
            {
                "ground_truth": expected.strip(),
                "best_candidate": best,
                "latex_similarity": round(SequenceMatcher(None, target, best).ratio(), 6),
                "exact_latex_match": target == best,
                "best_structural_candidate": best_structural,
                "structural_token_similarity_diagnostic": round(
                    SequenceMatcher(
                        None, structural_target, diagnostic_tokens(best_structural)
                    ).ratio(),
                    6,
                ),
            }
        )
    return {
        "sample": sample,
        "pdf_sha256": sha256(pdf),
        "ground_truth_sha256": sha256(ground_truth),
        "ir_sha256": sha256(result),
        "pages": len(pages),
        "ground_truth_display_formulas": len(display_formulas),
        "recognized_formula_blocks": len(recognized),
        "exact_latex_matches": sum(item["exact_latex_match"] for item in comparisons),
        "formula_comparisons": comparisons,
        "display_formula_detection_recall_upper_bound": (
            min(len(recognized), len(display_formulas)) / len(display_formulas)
            if display_formulas
            else None
        ),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("fixture_dir", type=Path)
    parser.add_argument("samples", nargs="+", help="sample stems, such as sample-1")
    parser.add_argument("--ir-suffix", default=".ir.json")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    report = [score(args.fixture_dir, name, args.ir_suffix) for name in args.samples]
    output = json.dumps(report, ensure_ascii=False, indent=2)
    if args.output:
        args.output.write_text(output + "\n", encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()
