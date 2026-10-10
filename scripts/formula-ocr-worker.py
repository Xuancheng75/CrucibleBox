"""Optional local formula worker for the Document Engine JSON-line protocol.

Run with a managed Python environment containing PaddlePaddle and
PaddleOCR[doc-parser]. All model output goes to stderr; stdout is protocol-only.
"""

import contextlib
import hashlib
import json
import os
import sys
import tempfile
from pathlib import Path

sys.stdout.reconfigure(encoding="utf-8", errors="strict")
sys.stderr.reconfigure(encoding="utf-8", errors="replace")


cache_root = Path(
    os.environ.get(
        "CRUCIBLEBOX_MODEL_CACHE",
        str(Path(os.environ.get("LOCALAPPDATA", tempfile.gettempdir())) / "CrucibleBox" / "formula-cache"),
    )
)
os.environ.setdefault("PADDLE_PDX_CACHE_HOME", str(cache_root / "paddlex"))
os.environ.setdefault("MODELSCOPE_CACHE", str(cache_root / "modelscope"))
os.environ.setdefault("HF_HOME", str(cache_root / "huggingface"))
os.environ.setdefault("DISABLE_MODEL_SOURCE_CHECK", "True")

def frame(request_id, frame_type, **fields):
    response = {"protocolVersion": 1, "requestId": request_id, "type": frame_type, **fields}
    print(json.dumps(response, ensure_ascii=False, separators=(",", ":")), flush=True)


def load_models(formula_model, directory):
    expected = {
        "PP-DocLayout-M": "f374bb0269d91ab2eed393a5a2da6d73d98896ac5eeae4f9f585eba19bcbba74",
        "PP-FormulaNet_plus-S": "e464f94412feaa98f8791eacc84684f887b3569e30e80c52b8112e9cf7d4069b",
        "PP-FormulaNet_plus-L": "4245c39c181d1d21e472bc85c7434df9b23f177be46552c0542bf153addbc355",
        "UniMERNet": "3e27e304d2d986df7e82792555d2b0f2706211f79cb8084989b9696304130e9f",
    }
    for name in ("PP-DocLayout-M", formula_model):
        path = directory / "paddle-formula" / name / "inference.pdiparams"
        if not path.is_file():
            raise FileNotFoundError(f"Paddle formula model missing: {name}")
        if name in expected:
            with path.open("rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
            if digest != expected[name]:
                raise ValueError(f"Paddle formula model hash mismatch: {name}")
    with contextlib.redirect_stdout(sys.stderr):
        from PIL import Image
        from paddleocr import FormulaRecognition, LayoutDetection

        layout_path = directory / "paddle-formula" / "PP-DocLayout-M"
        formula_path = directory / "paddle-formula" / formula_model
        detector = LayoutDetection(
            model_name="PP-DocLayout-M",
            **({"model_dir": str(layout_path)} if layout_path.is_dir() else {}),
            device="cpu", enable_mkldnn=False, cpu_threads=2
        )
        recognizer = FormulaRecognition(
            model_name=formula_model,
            **({"model_dir": str(formula_path)} if formula_path.is_dir() else {}),
            device="cpu", enable_mkldnn=False, cpu_threads=2
        )
    return Image, detector, recognizer, formula_model


def process(request, models, request_id):
    image_class, detector, recognizer, formula_model = models
    input_path = Path(request["input"])
    if not input_path.is_file():
        raise ValueError("input image does not exist")
    if input_path.suffix.lower() not in {".png", ".jpg", ".jpeg", ".webp", ".bmp"}:
        raise ValueError("unsupported image format")
    image = image_class.open(input_path).convert("RGB")
    with contextlib.redirect_stdout(sys.stderr):
        boxes = list(detector.predict(str(input_path)))[0].json["res"]["boxes"]
    candidates = [box for box in boxes if box["label"] == "formula" and box["score"] >= 0.5]
    if len(candidates) > 256:
        raise ValueError("formula candidate budget exceeded")
    output = []
    with tempfile.TemporaryDirectory(prefix="cruciblebox-formula-") as temp:
        for index, candidate in enumerate(candidates):
            left, top, right, bottom = candidate["coordinate"]
            pad = 8
            rect = [
                max(0, int(left) - pad),
                max(0, int(top) - pad),
                min(image.width, int(right) + pad),
                min(image.height, int(bottom) + pad),
            ]
            if rect[2] <= rect[0] or rect[3] <= rect[1]:
                continue
            crop_path = Path(temp) / f"formula-{index}.png"
            image.crop(tuple(rect)).save(crop_path)
            with contextlib.redirect_stdout(sys.stderr):
                latex = list(recognizer.predict(str(crop_path)))[0].json["res"].get(
                    "rec_formula", ""
                )
            frame(
                request_id,
                "progress",
                stage="formula",
                percent=10 + int(80 * (index + 1) / max(1, len(candidates))),
                message=f"识别公式 {index + 1}/{len(candidates)}",
            )
            if not latex or len(latex) > 16_384:
                continue
            bbox = [int(left), int(top), int(right), int(bottom)]
            output.append(
                {
                    "type": "formula",
                    "text": latex,
                    "bbox": bbox,
                    "polygon": [
                        [bbox[0], bbox[1]],
                        [bbox[2], bbox[1]],
                        [bbox[2], bbox[3]],
                        [bbox[0], bbox[3]],
                    ],
                    "confidence": candidate["score"],
                    "formulaEngine": f"PP-DocLayout-M+{formula_model}",
                    "modelVersion": formula_model,
                }
            )
    return output


def main():
    models = None
    for line in sys.stdin:
        request_id = "unknown"
        try:
            if len(line) > 65_536:
                raise ValueError("request line budget exceeded")
            request = json.loads(line)
            request_id = request.get("requestId", "unknown")
            if (
                request.get("protocolVersion") != 1
                or request.get("task") != "ocr"
                or not isinstance(request_id, str)
                or not request_id
                or len(request_id) > 128
            ):
                raise ValueError("invalid formula worker request")
            if models is None:
                profile = request.get("options", {}).get("modelProfile")
                formula_model = {
                    "pp-doclayout-m-formulanet-s": "PP-FormulaNet-S",
                    "pp-doclayout-m-formulanet-plus-s": "PP-FormulaNet_plus-S",
                    "pp-doclayout-m-formulanet-plus-l": "PP-FormulaNet_plus-L",
                    "pp-doclayout-m-unimernet": "UniMERNet",
                }.get(profile)
                if formula_model is None:
                    raise ValueError("unsupported formula model profile")
                frame(request_id, "progress", stage="model", percent=1, message="加载公式模型")
                directory = Path(request["options"]["modelDirectory"])
                models = load_models(formula_model, directory)
            elif request.get("options", {}).get("modelProfile") != {
                "PP-FormulaNet-S": "pp-doclayout-m-formulanet-s",
                "PP-FormulaNet_plus-S": "pp-doclayout-m-formulanet-plus-s",
                "PP-FormulaNet_plus-L": "pp-doclayout-m-formulanet-plus-l",
                "UniMERNet": "pp-doclayout-m-unimernet",
            }[models[3]]:
                raise ValueError("formula model profile changed within worker")
            blocks = process(request, models, request_id)
            frame(request_id, "result", text="\n".join(b["text"] for b in blocks), blocks=blocks)
        except Exception as error:
            frame(request_id, "error", code="formula-worker-failed", error=str(error))


if __name__ == "__main__":
    main()
