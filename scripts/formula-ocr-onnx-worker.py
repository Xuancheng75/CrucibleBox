"""Low-memory formula worker using ONNX layout and RapidLaTeXOCR models.

The six pinned model files live in <modelDirectory>/formula-onnx. The worker
speaks the same newline-delimited protocol as the Rust OCR worker.
"""

import contextlib
import hashlib
import json
import os
import sys
import types
from pathlib import Path

os.environ.setdefault("OMP_NUM_THREADS", "2")
sys.stdout.reconfigure(encoding="utf-8", errors="strict")
sys.stderr.reconfigure(encoding="utf-8", errors="replace")

MODEL_HASHES = {
    "PP-DocLayout-M.onnx": "34ecdc84e60d5f5822fab85e3e97f30ca73fdc6c7c0aa10b3bd7227742a574b5",
    "decoder.onnx": "bd695497bf1b22279b7626f5916c79226e1e244c84355f8da7edfd2d921d0072",
    "encoder.onnx": "01bf5dc25539ca0cd5b1bd29296ea495977a6ba5f629dc4178277809d26e5e7d",
    "image_resizer.onnx": "e0b075c39700f64d50400f39c8fc186bbb3b5d84d31864008313f376603aca9d",
    "tokenizer.json": "1dc27b18d6a518d0d5ff3f4bb7bd98521fe80ad39e5b2a246d4109f1bb9d5019",
}
TEXT_MODEL_HASHES = {
    "ppocrv6_small_det.onnx": "090f04abcd9d9a7498bc4ebf677e4cb9bdce1fe4197ddb7e529f1ef44e1ff94f",
    "PP-OCRv5_mobile_rec.onnx": "5825fc7ebf84ae7a412be049820b4d86d77620f204a041697b0494669b1742c5",
    "ppocrv5_dict.txt": "d1979e9f794c464c0d2e0b70a7fe14dd978e9dc644c0e71f14158cdf8342af1b",
    "en_PP-OCRv5_mobile_rec.onnx": "b5f833dfc5d0eb71da397b4efa06ebeee9b431b690a47d6af40d77d8eabc557f",
    "en_ppocrv5_dict.txt": "e025a66d31f327ba0c232e03f407ae8d105e1e709e7ccb3f408aa778c24e70d6",
}
PP_FORMULANET_HASH = "30998d10c94ccff1ad8981df0c71048cb1f3eec7b1e515b809767f1f72aebe3b"
PP_PROFILE = "onnx-doclayout-m-formulanet-plus-s"
RAPID_PROFILE = "onnx-doclayout-m-rapidlatex"
FAST_PROFILE = "onnx-text-fast"


def frame(request_id, frame_type, **fields):
    response = {"protocolVersion": 1, "requestId": request_id, "type": frame_type, **fields}
    print(json.dumps(response, ensure_ascii=False, separators=(",", ":")), flush=True)


def validate_models(directory, profile, language):
    required = {} if profile == FAST_PROFILE else {"PP-DocLayout-M.onnx": MODEL_HASHES["PP-DocLayout-M.onnx"]}
    if profile == RAPID_PROFILE:
        required = MODEL_HASHES
    elif profile == PP_PROFILE:
        required["pp_formulanet_plus_s.onnx"] = PP_FORMULANET_HASH
    for name, expected in required.items():
        path = directory / name
        if not path.is_file():
            raise FileNotFoundError(f"formula model missing: {name}")
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        if digest != expected:
            raise ValueError(f"formula model hash mismatch: {name}")
    text_names = ["ppocrv6_small_det.onnx"]
    text_names += ["en_PP-OCRv5_mobile_rec.onnx", "en_ppocrv5_dict.txt"] if language == "en" else [
        "PP-OCRv5_mobile_rec.onnx", "ppocrv5_dict.txt"
    ]
    for name in text_names:
        path = directory.parent / name
        if not path.is_file():
            raise FileNotFoundError(f"text model missing: {name}")
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        if digest != TEXT_MODEL_HASHES[name]:
            raise ValueError(f"text model hash mismatch: {name}")


def load_pp_formulanet(directory):
    path = directory / "pp_formulanet_plus_s.onnx"
    # RapidDoc's package entry point imports unrelated document converters.
    # Register only its formula package hierarchy to keep the worker lean.
    rapid_root = Path(sys.prefix) / "Lib" / "site-packages" / "rapid_doc"
    for name in (
        "rapid_doc",
        "rapid_doc.model",
        "rapid_doc.model.formula",
        "rapid_doc.model.formula.rapid_formula_self",
    ):
        module = types.ModuleType(name)
        module.__path__ = [str(rapid_root.joinpath(*name.split(".")[1:]))]
        sys.modules[name] = module
    from rapid_doc.model.formula.rapid_formula_self.main import RapidFormula
    from rapid_doc.model.formula.rapid_formula_self.utils.typings import ModelType, RapidFormulaInput

    return RapidFormula(RapidFormulaInput(model_type=ModelType.PP_FORMULANET_PLUS_S, model_dir_or_path=path))


def load_models(directory, profile, language):
    validate_models(directory, profile, language)
    with contextlib.redirect_stdout(sys.stderr):
        import cv2
        import numpy as np
        import onnxruntime as ort
        from rapidocr import RapidOCR

        cv2.setNumThreads(1)
        options = ort.SessionOptions()
        options.intra_op_num_threads = 2
        options.inter_op_num_threads = 1
        layout = None if profile == FAST_PROFILE else ort.InferenceSession(
            str(directory / "PP-DocLayout-M.onnx"),
            sess_options=options,
            providers=["CPUExecutionProvider"],
        )
        if profile == PP_PROFILE:
            formula = load_pp_formulanet(directory)
        elif profile == RAPID_PROFILE:
            from rapid_latex_ocr import LaTeXOCR

            formula = LaTeXOCR(
                image_resizer_path=directory / "image_resizer.onnx",
                encoder_path=directory / "encoder.onnx",
                decoder_path=directory / "decoder.onnx",
                tokenizer_json=directory / "tokenizer.json",
            )
        else:
            formula = None
        model_root = directory.parent
        english = language == "en"
        text = RapidOCR(params={
            "Global.use_cls": False,
            "Det.model_path": str(model_root / "ppocrv6_small_det.onnx"),
            "Rec.model_path": str(model_root / ("en_PP-OCRv5_mobile_rec.onnx" if english else "PP-OCRv5_mobile_rec.onnx")),
            "Rec.rec_keys_path": str(model_root / ("en_ppocrv5_dict.txt" if english else "ppocrv5_dict.txt")),
        })
    return cv2, np, layout, formula, text


def process(request, models, profile):
    cv2, np, layout, formula, text_engine = models
    path = Path(request["input"])
    if not path.is_file() or path.suffix.lower() not in {".png", ".jpg", ".jpeg", ".webp", ".bmp", ".tif", ".tiff"}:
        raise ValueError("formula input image is unavailable")
    image = cv2.imread(str(path))
    if image is None:
        raise ValueError("formula input cannot be decoded")
    height, width = image.shape[:2]
    if height * width > 40_000_000:
        raise ValueError("formula input exceeds image budget")
    with contextlib.redirect_stdout(sys.stderr):
        text_result = text_engine(image)
    output = []
    if text_result.boxes is not None and text_result.txts is not None:
        for polygon, content, score in zip(text_result.boxes, text_result.txts, text_result.scores):
            points = [[int(x), int(y)] for x, y in polygon]
            bbox = [min(p[0] for p in points), min(p[1] for p in points), max(p[0] for p in points), max(p[1] for p in points)]
            if content and len(content) <= 16_384:
                output.append({"type": "text", "text": content, "bbox": bbox, "polygon": points, "confidence": float(score)})
    if profile == FAST_PROFILE:
        return output
    rgb = cv2.cvtColor(image, cv2.COLOR_BGR2RGB)
    resized = cv2.resize(rgb, (640, 640), interpolation=cv2.INTER_CUBIC)
    tensor = resized.astype("float32") / 255
    tensor = (tensor - np.array([0.485, 0.456, 0.406], dtype="float32")) / np.array(
        [0.229, 0.224, 0.225], dtype="float32"
    )
    tensor = np.transpose(tensor, (2, 0, 1))[None]
    scale = np.array([[640 / height, 640 / width]], dtype="float32")
    with contextlib.redirect_stdout(sys.stderr):
        boxes, count = layout.run(None, {"image": tensor, "scale_factor": scale})
    candidates = [
        box for box in boxes[: int(count[0])] if int(box[0]) == 7 and float(box[1]) >= 0.5
    ]
    if len(candidates) > 256:
        raise ValueError("formula candidate budget exceeded")
    for box in candidates:
        left, top, right, bottom = map(float, box[2:6])
        pad = 8
        x1, y1 = max(0, int(left) - pad), max(0, int(top) - pad)
        x2, y2 = min(width, int(right) + pad), min(height, int(bottom) + pad)
        if x2 <= x1 or y2 <= y1:
            continue
        with contextlib.redirect_stdout(sys.stderr):
            if profile == PP_PROFILE:
                latex = formula([image[y1:y2, x1:x2]])[0].rec_formula
            else:
                success, encoded = cv2.imencode(".png", image[y1:y2, x1:x2])
                if not success:
                    continue
                latex, _ = formula(encoded.tobytes())
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
                "confidence": float(box[1]),
                "formulaEngine": "PP-DocLayout-M-ONNX+" + ("PP-FormulaNet_plus-S" if profile == PP_PROFILE else "RapidLaTeXOCR"),
                "modelVersion": "RapidDoc-0.9.10" if profile == PP_PROFILE else "RapidLaTeXOCR-0.0.9",
            }
        )
    formulas = [block["bbox"] for block in output if block["type"] == "formula"]
    def covered_by_formula(block):
        if block["type"] != "text":
            return False
        left, top, right, bottom = block["bbox"]
        area = max(1, (right - left) * (bottom - top))
        for f_left, f_top, f_right, f_bottom in formulas:
            overlap = max(0, min(right, f_right) - max(left, f_left)) * max(
                0, min(bottom, f_bottom) - max(top, f_top)
            )
            if overlap / area > 0.5:
                return True
        return False
    return [block for block in output if not covered_by_formula(block)]


def main():
    models = None
    active_directory = None
    active_profile = None
    active_language = None
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
                or request.get("options", {}).get("modelProfile") not in {FAST_PROFILE, RAPID_PROFILE, PP_PROFILE}
            ):
                raise ValueError("invalid formula worker request")
            directory = Path(request["options"]["modelDirectory"]) / "formula-onnx"
            profile = request["options"]["modelProfile"]
            language = request["options"].get("language", "mix")
            if language not in {"auto", "zh", "en", "mix"}:
                raise ValueError("unsupported text recognition language")
            if models is None:
                frame(request_id, "progress", stage="model", percent=1, message="加载公式模型")
                models = load_models(directory, profile, language)
                active_directory = directory
                active_profile = profile
                active_language = language
            elif directory != active_directory or profile != active_profile or language != active_language:
                raise ValueError("formula model directory changed within worker")
            blocks = process(request, models, profile)
            english = language == "en"
            frame(
                request_id,
                "result",
                text="\n".join(b["text"] for b in blocks if b["type"] == "text"),
                blocks=blocks,
                model={
                    "modelProfile": profile,
                    "detectionSha256": TEXT_MODEL_HASHES["ppocrv6_small_det.onnx"],
                    "recognitionSha256": TEXT_MODEL_HASHES[
                        "en_PP-OCRv5_mobile_rec.onnx" if english else "PP-OCRv5_mobile_rec.onnx"
                    ],
                    "dictionarySha256": TEXT_MODEL_HASHES[
                        "en_ppocrv5_dict.txt" if english else "ppocrv5_dict.txt"
                    ],
                    "device": "cpu",
                },
            )
        except Exception as error:
            frame(request_id, "error", code="formula-worker-failed", error=str(error))


if __name__ == "__main__":
    main()
