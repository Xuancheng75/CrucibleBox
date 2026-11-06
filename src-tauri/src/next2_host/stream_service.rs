use base64::Engine;
use cruciblebox_next2_protocol as protocol;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

struct Reader {
    file: File,
    bytes: u64,
    modified: SystemTime,
}

struct Stream {
    owner: String,
    caller: Option<String>,
    session: String,
    expires: Instant,
    reader: Arc<Mutex<Reader>>,
}

#[derive(Default)]
pub(super) struct Streams(Mutex<HashMap<String, Stream>>);

fn lease(now: Instant) -> Instant {
    now + Duration::from_millis(protocol::BUDGET_LEASE_MS as u64)
}

impl Streams {
    pub(super) fn open(
        &self,
        owner: &str,
        caller: Option<&str>,
        session: &str,
        path: &Path,
    ) -> Result<Value, String> {
        let file = open_authorized(path)?;
        let metadata = file.metadata().map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("FILE_AUTHORITY_DENIED".into());
        }
        if !metadata.is_file() {
            return Err("FILE_REQUIRED".into());
        }
        let bytes = metadata.len();
        if bytes > 16 * 1024 * 1024 * 1024 {
            return Err("FILE_BUDGET_EXCEEDED".into());
        }
        let reader = Reader {
            file,
            bytes,
            modified: metadata.modified().map_err(|error| error.to_string())?,
        };
        let now = Instant::now();
        let mut streams = self.0.lock().map_err(|_| "FILE_STREAMS_UNAVAILABLE")?;
        streams.retain(|_, stream| now < stream.expires);
        if streams.len() >= protocol::BUDGET_FILE_STREAMS
            || streams
                .values()
                .filter(|item| item.session == session)
                .count()
                >= protocol::BUDGET_FILE_STREAMS_PER_SESSION
        {
            return Err("FILE_STREAM_LIMIT".into());
        }
        let id = crate::rand_token::random_token_hex()?;
        streams.insert(
            id.clone(),
            Stream {
                owner: owner.into(),
                caller: caller.map(str::to_owned),
                session: session.into(),
                expires: lease(now),
                reader: Arc::new(Mutex::new(reader)),
            },
        );
        Ok(json!({"streamId":id,"bytes":bytes}))
    }

    pub(super) fn read(
        &self,
        session: &str,
        id: &str,
        offset: u64,
        length: u64,
    ) -> Result<Value, String> {
        let now = Instant::now();
        let reader = {
            let mut streams = self.0.lock().map_err(|_| "FILE_STREAMS_UNAVAILABLE")?;
            let item = streams
                .get_mut(id)
                .filter(|item| item.session == session && now < item.expires)
                .ok_or("FILE_STREAM_UNAVAILABLE")?;
            item.expires = lease(now);
            item.reader.clone()
        };
        let result = reader
            .lock()
            .map_err(|_| "FILE_STREAMS_UNAVAILABLE")?
            .read(offset, length);
        if result
            .as_ref()
            .is_err_and(|error| error == "FILE_STREAM_CHANGED")
        {
            self.close(session, id)?;
        }
        result
    }

    pub(super) fn close(&self, session: &str, id: &str) -> Result<Value, String> {
        let mut streams = self.0.lock().map_err(|_| "FILE_STREAMS_UNAVAILABLE")?;
        if streams.get(id).is_some_and(|item| item.session == session) {
            streams.remove(id);
            Ok(json!(true))
        } else {
            Ok(json!(false))
        }
    }

    pub(super) fn clear_session(&self, session: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "FILE_STREAMS_UNAVAILABLE")?
            .retain(|_, item| item.session != session);
        Ok(())
    }

    pub(super) fn clear_plugin(&self, owner: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "FILE_STREAMS_UNAVAILABLE")?
            .retain(|_, item| item.owner != owner && item.caller.as_deref() != Some(owner));
        Ok(())
    }
}

impl Reader {
    fn check(&self) -> Result<(), String> {
        let metadata = self.file.metadata().map_err(|error| error.to_string())?;
        if metadata.len() != self.bytes
            || metadata.modified().map_err(|error| error.to_string())? != self.modified
        {
            return Err("FILE_STREAM_CHANGED".into());
        }
        Ok(())
    }

    fn read(&mut self, offset: u64, length: u64) -> Result<Value, String> {
        if offset > self.bytes {
            return Err("FILE_OFFSET_OUT_OF_RANGE".into());
        }
        self.check()?;
        let mut chunk = vec![0; length.min(self.bytes - offset) as usize];
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        self.file.read_exact(&mut chunk).map_err(|error| {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                "FILE_STREAM_CHANGED".into()
            } else {
                error.to_string()
            }
        })?;
        self.check()?;
        let next = offset + chunk.len() as u64;
        Ok(
            json!({"data":base64::engine::general_purpose::STANDARD.encode(chunk),"offset":offset,"nextOffset":next,"eof":next==self.bytes}),
        )
    }
}

fn open_authorized(path: &Path) -> Result<File, String> {
    use std::{
        ffi::OsString,
        os::windows::{ffi::OsStringExt, fs::OpenOptionsExt, io::AsRawHandle},
    };
    use windows_sys::Win32::Storage::FileSystem::{
        GetFinalPathNameByHandleW, FILE_FLAG_OPEN_REPARSE_POINT,
    };

    // Both selection grants and persisted publication receipts store the
    // canonical path from the time access was granted.
    let file = File::options()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| error.to_string())?;
    let mut buffer = vec![0; 32768];
    let size = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle() as _,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    } as usize;
    if size == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    if size >= buffer.len() || Path::new(&OsString::from_wide(&buffer[..size])) != path {
        return Err("FILE_AUTHORITY_DENIED".into());
    }
    Ok(file)
}
