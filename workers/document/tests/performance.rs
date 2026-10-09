#[cfg(windows)]
mod windows_performance {
    use cruciblebox_document_native::pdf_parser;
    use cruciblebox_document_worker::{client::Client, protocol::Operation};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        path::Path,
        process::Command,
        time::{Duration, Instant},
    };

    fn fixture(page_count: usize, lines_per_page: usize) -> Vec<u8> {
        let font_id = 3 + page_count * 2;
        let mut objects = vec![String::new(); font_id + 1];
        objects[1] = "<< /Type /Catalog /Pages 2 0 R >>".into();
        let page_ids = (0..page_count)
            .map(|i| format!("{} 0 R", 3 + i * 2))
            .collect::<Vec<_>>()
            .join(" ");
        objects[2] = format!("<< /Type /Pages /Kids [{page_ids}] /Count {page_count} >>");
        for page in 0..page_count {
            let page_id = 3 + page * 2;
            let content_id = page_id + 1;
            objects[page_id] = format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 {font_id} 0 R >> >> /Contents {content_id} 0 R >>");
            let mut content = String::from("BT\n/F1 9 Tf\n40 760 Td\n");
            for line in 0..lines_per_page {
                content.push_str(&format!(
                    "(Page {:02} line {:03} deterministic worker benchmark text) Tj\n0 -11 Td\n",
                    page + 1,
                    line + 1
                ));
            }
            content.push_str("ET\n");
            objects[content_id] = format!(
                "<< /Length {} >>\nstream\n{}endstream",
                content.len(),
                content
            );
        }
        objects[font_id] = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into();

        let mut bytes = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = vec![0usize; objects.len()];
        for id in 1..objects.len() {
            offsets[id] = bytes.len();
            bytes.extend_from_slice(format!("{id} 0 obj\n{}\nendobj\n", objects[id]).as_bytes());
        }
        let xref = bytes.len();
        bytes.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objects.len()).as_bytes(),
        );
        for offset in offsets.iter().skip(1) {
            bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        bytes.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len()
            )
            .as_bytes(),
        );
        bytes
    }

    fn semantic_signature(value: &serde_json::Value) -> serde_json::Value {
        let pages = value["document"]["pages"]
            .as_array()
            .expect("parsed document pages")
            .iter()
            .map(|page| {
                page["blocks"]
                    .as_array()
                    .expect("page blocks")
                    .iter()
                    .map(|block| json!({"type": block["type"], "content": block["content"]}))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        json!({
            "route": value["route"],
            "pageCount": value["document"]["metadata"]["pageCount"],
            "ocrPageNumbers": value["ocrPageNumbers"],
            "pages": pages
        })
    }

    fn median(values: &[f64]) -> f64 {
        let mut values = values.to_vec();
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    }

    #[test]
    #[ignore = "controlled 2000-page worker stress acceptance with real PDFium"]
    fn two_thousand_page_pdf_completes_without_leaking_job_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let pdfium =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/pdfium.dll");
        let input = root.path().join("synthetic-2000-page.pdf");
        let bytes = fixture(2000, 4);
        fs::write(&input, &bytes).unwrap();
        let jobs = root.path().join("jobs");
        let worker = Client::new(
            env!("CARGO_BIN_EXE_document-worker").into(),
            Some(pdfium),
            jobs.clone(),
            Duration::from_secs(180),
        );
        let started = Instant::now();
        let result = worker
            .run(
                &Operation::Parse {
                    path: input.to_string_lossy().into_owned(),
                },
                &|| false,
            )
            .unwrap();
        assert_eq!(result.value["document"]["metadata"]["pageCount"], 2000);
        let pages = result.value["document"]["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 2000);
        assert!(pages[1999]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|block| block["content"]
                .as_str()
                .is_some_and(|text| text.contains("Page 2000"))));
        let report = json!({
            "scope": "synthetic 2000-page native text PDF; not OCR or whole-application performance",
            "pages": 2000,
            "inputBytes": bytes.len(),
            "elapsedMs": started.elapsed().as_secs_f64() * 1000.0,
            "route": result.value["route"],
            "lastPageVerified": true
        });
        drop(result);
        assert_eq!(fs::read_dir(&jobs).unwrap().count(), 0);
        eprintln!("{report}");
        if let Some(path) = std::env::var_os("DOCUMENT_WORKER_STRESS_RESULT") {
            fs::write(path, serde_json::to_string_pretty(&report).unwrap() + "\n").unwrap();
        }
    }
    #[test]
    #[ignore = "controlled Windows-only direct-vs-worker measurement; requires the pinned PDFium runtime"]
    fn direct_and_worker_pdf_parse_performance_comparison() {
        let root = tempfile::tempdir().unwrap();
        let pdfium =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src-tauri/resources/pdfium.dll");
        assert!(pdfium.is_file(), "pinned PDFium DLL is required");
        std::env::set_var("PDFIUM_LIB_PATH", &pdfium);
        let input = root.path().join("synthetic-10-page.pdf");
        let bytes = fixture(10, 64);
        fs::write(&input, &bytes).unwrap();
        let input_path = input.to_string_lossy().into_owned();
        let worker = Client::new(
            env!("CARGO_BIN_EXE_document-worker").into(),
            Some(pdfium),
            root.path().join("jobs"),
            Duration::from_secs(30),
        );
        let direct_warm = pdf_parser::parse_file(&input_path).unwrap();
        let worker_warm = worker
            .run(
                &Operation::Parse {
                    path: input_path.clone(),
                },
                &|| false,
            )
            .unwrap();
        assert_eq!(
            semantic_signature(&direct_warm),
            semantic_signature(&worker_warm.value),
            "both paths must return the same route, page count, OCR pages, block types, and text"
        );
        drop(worker_warm);

        let mut direct_ms = Vec::new();
        let mut worker_ms = Vec::new();
        for batch in 0..3 {
            for iteration in 0..15 {
                if (batch + iteration) % 2 == 0 {
                    let started = Instant::now();
                    let direct = pdf_parser::parse_file(&input_path).unwrap();
                    direct_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                    let started = Instant::now();
                    let isolated = worker
                        .run(
                            &Operation::Parse {
                                path: input_path.clone(),
                            },
                            &|| false,
                        )
                        .unwrap();
                    worker_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                    assert_eq!(
                        semantic_signature(&direct),
                        semantic_signature(&isolated.value)
                    );
                } else {
                    let started = Instant::now();
                    let isolated = worker
                        .run(
                            &Operation::Parse {
                                path: input_path.clone(),
                            },
                            &|| false,
                        )
                        .unwrap();
                    worker_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                    let started = Instant::now();
                    let direct = pdf_parser::parse_file(&input_path).unwrap();
                    direct_ms.push(started.elapsed().as_secs_f64() * 1000.0);
                    assert_eq!(
                        semantic_signature(&direct),
                        semantic_signature(&isolated.value)
                    );
                }
            }
        }
        let summarize = |samples: &[f64]| {
            json!({
                "samples": samples.len(),
                "medianMs": median(samples),
                "minMs": samples.iter().copied().fold(f64::INFINITY, f64::min),
                "maxMs": samples.iter().copied().fold(0.0, f64::max)
            })
        };
        let report = json!({
            "comparison": "same PDFium parser and same synthetic PDF; direct in-process call versus isolated process plus reference IPC",
            "scope": "parse latency only; not whole-application, real-document, memory, OCR, or launch performance",
            "fixture": {
                "pages": 10,
                "linesPerPage": 64,
                "bytes": bytes.len(),
                "sha256": format!("{:x}", Sha256::digest(&bytes))
            },
            "toolchain": Command::new("rustc").arg("-vV").output().ok().map(|v| String::from_utf8_lossy(&v.stdout).into_owned()),
            "direct": summarize(&direct_ms),
            "worker": summarize(&worker_ms),
            "medianWorkerOverheadMs": median(&worker_ms) - median(&direct_ms)
        });
        let serialized = serde_json::to_string_pretty(&report).unwrap();
        eprintln!("{serialized}");
        if let Some(path) = std::env::var_os("DOCUMENT_WORKER_PERF_RESULT") {
            let path = Path::new(&path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, serialized + "\n").unwrap();
        }
    }
}
