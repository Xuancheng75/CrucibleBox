//! Native transport used by the marketplace catalog request.
//!
//! The marketplace uses direct HTTPS to the first-party GitHub release
//! address. It does not inherit a shell proxy or a mirror endpoint.

#[cfg(windows)]
pub fn get_text(url: &str, max_bytes: u64) -> Result<String, String> {
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use windows_sys::Win32::Networking::WinHttp::{
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
        WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
        WinHttpSendRequest, WinHttpSetTimeouts, HTTP_STATUS_OK, WINHTTP_ACCESS_TYPE_NO_PROXY,
        WINHTTP_FLAG_SECURE, WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
    };

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn close(handle: *mut c_void) {
        if !handle.is_null() {
            unsafe {
                WinHttpCloseHandle(handle);
            }
        }
    }

    let (host, path) = url
        .strip_prefix("https://")
        .and_then(|rest| {
            let split_at = rest.find('/').unwrap_or(rest.len());
            let (host, suffix) = rest.split_at(split_at);
            (!host.is_empty()).then_some((host, if suffix.is_empty() { "/" } else { suffix }))
        })
        .ok_or_else(|| "WinHTTP 只允许解析 HTTPS 地址".to_string())?;
    if host.contains(':') {
        return Err("WinHTTP 不接受带端口的目录地址".into());
    }

    let agent = wide(concat!("CrucibleBox/", env!("CARGO_PKG_VERSION")));
    let host = wide(host);
    let path = wide(path);
    let verb = wide("GET");
    let version = wide("HTTP/1.1");
    let user_agent_header = wide(concat!(
        "Accept: application/json\r\n",
        "Cache-Control: no-cache\r\n"
    ));

    unsafe {
        let session = WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            std::ptr::null(),
            std::ptr::null(),
            0,
        );
        if session.is_null() {
            return Err("WinHTTP 初始化会话失败".into());
        }
        let result = (|| -> Result<String, String> {
            if WinHttpSetTimeouts(session, 8_000, 15_000, 15_000, 20_000) == 0 {
                return Err("WinHTTP 设置超时失败".into());
            }
            let connection = WinHttpConnect(session, host.as_ptr(), 443, 0);
            if connection.is_null() {
                return Err("WinHTTP 连接 GitHub 失败".into());
            }
            let request = WinHttpOpenRequest(
                connection,
                verb.as_ptr(),
                path.as_ptr(),
                version.as_ptr(),
                std::ptr::null(),
                [std::ptr::null::<u16>()].as_ptr(),
                WINHTTP_FLAG_SECURE,
            );
            if request.is_null() {
                close(connection);
                return Err("WinHTTP 创建目录请求失败".into());
            }
            let request_result = (|| -> Result<String, String> {
                if WinHttpSendRequest(
                    request,
                    user_agent_header.as_ptr(),
                    (user_agent_header.len() - 1) as u32,
                    null_mut(),
                    0,
                    0,
                    0,
                ) == 0
                {
                    return Err("WinHTTP 发送目录请求失败".into());
                }
                if WinHttpReceiveResponse(request, null_mut()) == 0 {
                    return Err("WinHTTP 接收目录响应失败".into());
                }

                let mut status = 0_u32;
                let mut status_bytes = std::mem::size_of::<u32>() as u32;
                if WinHttpQueryHeaders(
                    request,
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    std::ptr::null(),
                    (&mut status as *mut u32).cast::<c_void>(),
                    &mut status_bytes,
                    null_mut(),
                ) == 0
                {
                    return Err("WinHTTP 读取目录响应状态失败".into());
                }
                if status != HTTP_STATUS_OK {
                    return Err(format!("WinHTTP 目录响应状态异常：{status}"));
                }

                let mut body = Vec::new();
                loop {
                    let mut available = 0_u32;
                    if WinHttpQueryDataAvailable(request, &mut available) == 0 {
                        return Err("WinHTTP 查询目录响应大小失败".into());
                    }
                    if available == 0 {
                        break;
                    }
                    let next_len = body.len().saturating_add(available as usize);
                    if next_len as u64 > max_bytes {
                        return Err("官方插件目录超过安全大小限制".into());
                    }
                    let start = body.len();
                    body.resize(next_len, 0);
                    let mut read = 0_u32;
                    if WinHttpReadData(
                        request,
                        body[start..].as_mut_ptr().cast::<c_void>(),
                        available,
                        &mut read,
                    ) == 0
                    {
                        return Err("WinHTTP 读取目录响应失败".into());
                    }
                    if read == 0 {
                        body.truncate(start);
                        break;
                    }
                    body.truncate(start + read as usize);
                }
                String::from_utf8(body).map_err(|error| format!("官方插件目录不是 UTF-8：{error}"))
            })();
            close(request);
            request_result
        })();
        close(session);
        result
    }
}

#[cfg(not(windows))]
pub fn get_text(_url: &str, _max_bytes: u64) -> Result<String, String> {
    Err("WinHTTP 仅支持 Windows".into())
}
