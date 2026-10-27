use cruciblebox_next2_runtime::transport::{read_frame, write_frame};
use serde_json::{json, Value};
use std::{io, time::Duration};

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--child") {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    let mut input = io::stdin();
    let mut output = io::stdout();
    if std::path::Path::new("wrong-wire.txt").exists() {
        write_frame(
            &mut output,
            &json!({"kind":"hello","wireVersion":3,"sdkApiVersion":6}),
        )
        .unwrap();
        return;
    }
    write_frame(
        &mut output,
        &json!({"kind":"hello","wireVersion":4,"sdkApiVersion":6}),
    )
    .unwrap();
    let initialize = read_frame(&mut input).unwrap().unwrap();
    assert_eq!(initialize["kind"], "initialize");
    let session = initialize["session"].as_str().unwrap();
    write_frame(&mut output, &json!({"kind":"ready","wireVersion":4})).unwrap();
    while let Some(frame) = read_frame(&mut input).unwrap() {
        if frame["kind"] == "stop" {
            return;
        }
        let request = &frame["request"];
        let mut result = Value::Null;
        let mut error = None;
        match request["method"].as_str().unwrap() {
            "sum" => {
                result = json!(request["params"]["numbers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_i64().unwrap())
                    .sum::<i64>())
            }
            "identity" => {
                write_frame(&mut output,&json!({"kind":"capability","request":{"wireVersion":4,"requestId":"cap-1",
                    "session":session,"target":{"kind":"platform"},"method":"storage.get","params":{"key":"fixture"}}})).unwrap();
                let response = read_frame(&mut input).unwrap().unwrap();
                result = json!({"caller":frame["callerId"],"value":response["response"]["result"]});
            }
            "wait-cancel" => {
                write_frame(&mut output,&json!({"kind":"capability","request":{"wireVersion":4,"requestId":"progress-1",
                    "session":session,"target":{"kind":"platform"},"method":"tasks.progress","params":{"taskId":frame["taskId"],
                    "sequence":1,"stage":"running","percent":25,"message":"Waiting for cancellation"}}})).unwrap();
                let response = read_frame(&mut input).unwrap().unwrap();
                assert_eq!(response["kind"], "capability-result");
                let cancel = read_frame(&mut input).unwrap().unwrap();
                assert_eq!(cancel["kind"], "cancel");
                error = Some("CANCELLED");
            }
            "crash" => std::process::exit(42),
            "hang-child" => {
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                command
                    .arg("--child")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                let mut child = command.spawn().unwrap();
                std::fs::write(
                    std::path::Path::new(initialize["dataDirectory"].as_str().unwrap())
                        .join("child.pid"),
                    child.id().to_string(),
                )
                .unwrap();
                let _ = child.wait();
                loop {
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
            _ => error = Some("METHOD_UNAVAILABLE"),
        }
        let response = match error {
            Some(code) => {
                json!({"wireVersion":4,"requestId":request["requestId"],"ok":false,"error":{"code":code,"message":code}})
            }
            None => {
                json!({"wireVersion":4,"requestId":request["requestId"],"ok":true,"result":result})
            }
        };
        write_frame(&mut output, &json!({"kind":"reply","response":response})).unwrap();
    }
}
