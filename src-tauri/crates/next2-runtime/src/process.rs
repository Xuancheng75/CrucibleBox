use std::{
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    process::{Child, Command},
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_INVALID_PARAMETER, ERROR_MORE_DATA, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
    },
    System::{Diagnostics::ToolHelp::*, JobObjects::*, Threading::*},
};

#[derive(Clone)]
pub struct StopHandle(Arc<OwnedHandle>);
impl StopHandle {
    pub fn wait_empty(&self) -> Result<(), String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
                unsafe { std::mem::zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    self.0.as_raw_handle() as _,
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as _,
                    std::mem::size_of_val(&accounting) as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(format!(
                    "PROCESS_TREE_WAIT_FAILED: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if accounting.ActiveProcesses == 0 {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err("PROCESS_TREE_STILL_RUNNING".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    pub fn terminate(&self) -> Result<(), String> {
        let processes = self.process_handles();
        if unsafe { TerminateJobObject(self.0.as_raw_handle() as _, 1) } == 0 {
            return Err(format!(
                "PROCESS_TREE_STOP_FAILED: {}",
                std::io::Error::last_os_error()
            ));
        }
        // Accounting can reach zero before the process object becomes signaled.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        for process in processes? {
            let timeout = deadline
                .saturating_duration_since(std::time::Instant::now())
                .as_millis()
                .min(5000) as u32;
            if unsafe { WaitForSingleObject(process.as_raw_handle() as _, timeout) }
                != WAIT_OBJECT_0
            {
                return Err("PROCESS_EXIT_NOT_CONFIRMED".into());
            }
        }
        self.wait_empty()?;
        Ok(())
    }
    fn process_handles(&self) -> Result<Vec<OwnedHandle>, String> {
        let mut buffer = vec![0usize; 17];
        loop {
            let success = unsafe {
                QueryInformationJobObject(
                    self.0.as_raw_handle() as _,
                    JobObjectBasicProcessIdList,
                    buffer.as_mut_ptr().cast(),
                    (buffer.len() * std::mem::size_of::<usize>()) as u32,
                    std::ptr::null_mut(),
                )
            };
            let list = unsafe { &*(buffer.as_ptr() as *const JOBOBJECT_BASIC_PROCESS_ID_LIST) };
            if success == 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(ERROR_MORE_DATA as i32) {
                    buffer.resize(list.NumberOfAssignedProcesses as usize + 17, 0);
                    continue;
                }
                return Err(format!(
                    "PROCESS_TREE_QUERY_FAILED: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let mut processes = Vec::new();
            for pid in &buffer[1..=list.NumberOfProcessIdsInList as usize] {
                let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, *pid as u32) };
                if handle.is_null() {
                    if std::io::Error::last_os_error().raw_os_error()
                        == Some(ERROR_INVALID_PARAMETER as i32)
                    {
                        continue;
                    }
                    return Err(format!(
                        "PROCESS_TREE_OPEN_FAILED: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                processes.push(unsafe { OwnedHandle::from_raw_handle(handle as _) });
            }
            return Ok(processes);
        }
    }
}
pub struct Process {
    pub child: Child,
    stop: StopHandle,
}
impl Process {
    /// Spawn suspended so the backend cannot create children before job assignment.
    pub fn spawn(command: &mut Command) -> Result<Self, String> {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child = command
            .spawn()
            .map_err(|e| format!("NATIVE_START_FAILED: {e}"))?;
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            let error = std::io::Error::last_os_error();
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("JOB_CREATE_FAILED: {error}"));
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(job as _) };
        let stop = StopHandle(Arc::new(handle));
        let mut process = Self { child, stop };
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as _,
                std::mem::size_of_val(&info) as u32,
            )
        } == 0
            || unsafe { AssignProcessToJobObject(job, process.child.as_raw_handle() as _) } == 0
        {
            return Err(format!(
                "JOB_ASSIGN_FAILED: {}",
                std::io::Error::last_os_error()
            ));
        }
        process.resume()?;
        Ok(process)
    }
    fn resume(&mut self) -> Result<(), String> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(format!(
                "THREAD_SNAPSHOT_FAILED: {}",
                std::io::Error::last_os_error()
            ));
        }
        let snapshot = unsafe { OwnedHandle::from_raw_handle(snapshot as _) };
        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of_val(&entry) as u32;
        let mut found = unsafe { Thread32First(snapshot.as_raw_handle() as _, &mut entry) };
        while found != 0 {
            if entry.th32OwnerProcessID == self.child.id() {
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                if thread.is_null() {
                    return Err(format!(
                        "THREAD_OPEN_FAILED: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                let result = unsafe { ResumeThread(thread) };
                unsafe { CloseHandle(thread) };
                if result == u32::MAX {
                    return Err(format!(
                        "THREAD_RESUME_FAILED: {}",
                        std::io::Error::last_os_error()
                    ));
                }
                return Ok(());
            }
            found = unsafe { Thread32Next(snapshot.as_raw_handle() as _, &mut entry) };
        }
        Err("PRIMARY_THREAD_NOT_FOUND".into())
    }
    pub fn stop_handle(&self) -> StopHandle {
        self.stop.clone()
    }
    pub fn force_stop(&mut self) -> Result<(), String> {
        self.stop.terminate()?;
        self.child
            .wait()
            .map_err(|e| format!("NATIVE_WAIT_FAILED: {e}"))?;
        self.stop.wait_empty()
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.stop.terminate();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
