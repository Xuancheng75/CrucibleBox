use cruciblebox_next2_native::{serve, Error};
use serde_json::json;

fn main() {
    if let Err(error) = serve(Ok, |client, call| match call.method.as_str() {
        "sum" => {
            let numbers = call.params["numbers"]
                .as_array()
                .ok_or_else(|| Error::new("INVALID_NUMBERS", "numbers must be an integer array"))?;
            let mut total = 0i64;
            for number in numbers {
                let number = number
                    .as_i64()
                    .ok_or_else(|| Error::new("INVALID_NUMBER", "Integer required"))?;
                total = total
                    .checked_add(number)
                    .ok_or_else(|| Error::new("OVERFLOW", "Sum exceeds i64"))?;
            }
            Ok(json!(total))
        }
        "identity" => Ok(
            json!({"caller":call.caller_id,"value":client.platform("storage.get",json!({"key":"identity"}))?}),
        ),
        "wait" | "wait-cancel" => {
            call.progress("work", 25, "Waiting for explicit cancellation")?;
            loop {
                call.check_cancelled()?;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        "crash" => std::process::exit(42),
        _ => Err(Error::new("METHOD_UNAVAILABLE", "Unknown business method")),
    }) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
