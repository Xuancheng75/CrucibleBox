//! Document parsing adapter. Native code and file decoding live in the worker.
use cruciblebox_document_worker::{client::Client, protocol::Operation};
pub fn parse_file(
    client: &Client,
    path: &str,
    ctx: &crate::task_runtime::Context,
) -> Result<serde_json::Value, String> {
    Ok(
        crate::document_worker::run(client, Operation::Parse { path: path.into() }, Some(ctx))?
            .value
            .clone(),
    )
}
