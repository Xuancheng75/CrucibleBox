//! Executor failure boundary shared by host task adapters.
use serde_json::Value;
pub fn run(execute: impl FnOnce() -> Result<Value, String>) -> Result<Value, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(execute))
        .unwrap_or_else(|_| Err("任务执行器异常退出；请查看诊断日志".into()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panic_is_diagnostic_failure_and_normal_error_is_preserved() {
        assert!(run(|| panic!("injected")).unwrap_err().contains("异常退出"));
        assert_eq!(run(|| Err("ordinary".into())).unwrap_err(), "ordinary");
        assert_eq!(
            run(|| Ok(serde_json::json!(1))).unwrap(),
            serde_json::json!(1)
        );
    }
}
