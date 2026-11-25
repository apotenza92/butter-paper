//! One running app per profile for PDFs opened from outside it. A launch
//! with PDFs to open first offers them to a running Butter Paper of the same
//! profile, which opens them as the Open Documents In setting says, and then
//! exits. Without a running app it becomes the one that listens. Windows and
//! Linux start a new process for each file the file manager opens, so this
//! is how those opens reach the running app; macOS delivers them to the
//! running app itself, and this covers its command line.
//!
//! The channel is private to the user, and each end checks the other:
//! - Unix: a socket in the user's runtime folder (Linux) or temporary folder
//!   (macOS), used only when that folder belongs to the user and no one else
//!   may enter it, so the moment between binding the socket and limiting its
//!   permissions to the user is closed to other users. The listener drops a
//!   peer of another user, and a launch talks only to a listener of its own
//!   user (the peer's credentials, from the kernel).
//! - Windows: a named pipe named with the user's SID, open only to the user
//!   and SYSTEM, refusing remote clients. A launch gives its PDFs only to a
//!   pipe whose server process belongs to the same user, so a pipe another
//!   user made first (squatting) gets nothing; this launch then runs as an
//!   app of its own.
//!
//! A request is a length and then PDF paths, one to a line; the listener
//! answers `OK` once it has them.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The first bytes of a request, with the protocol version.
const MAGIC: &[u8] = b"BUTTER-PAPER-OPEN 1\n";
/// The longest request the listener reads.
const MAX_REQUEST: usize = 1 << 20;
/// How long a launch waits for the running app to take its PDFs.
const FORWARD_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the Windows listener gives one launch to send its request and
/// read the reply. A launch sends at once, so this is shorter than
/// [`FORWARD_TIMEOUT`]: the next launch still gets its turn in time.
#[cfg(windows)]
const SERVE_TIMEOUT: Duration = Duration::from_secs(2);
const REPLY: &[u8] = b"OK\n";

/// Where a profile's running app listens.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Endpoint {
    #[cfg(unix)]
    socket: PathBuf,
    #[cfg(windows)]
    pipe: String,
    /// The current user's SID, as text: the only user the pipe admits.
    #[cfg(windows)]
    sid: String,
}

/// What this launch is, once it has looked for a running app.
pub enum InstanceRole {
    /// The running app took this launch's PDFs: exit.
    Forwarded,
    /// This launch runs the app. It listens for later launches when it
    /// could claim the channel (`None` when another app of the profile
    /// already listens, or the platform has no channel).
    Primary(Option<InstanceListener>),
}

/// The listening side of the channel, held by the running app.
pub struct InstanceListener {
    #[cfg(unix)]
    listener: std::os::unix::net::UnixListener,
    #[cfg(unix)]
    socket: PathBuf,
    #[cfg(windows)]
    first: windows::PipeHandle,
    #[cfg(windows)]
    pipe: String,
    #[cfg(windows)]
    sid: String,
}

/// A stable 64-bit FNV-1a hash, to name a profile's channel.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The channel name's profile part: a hash of the profile's folder, so each
/// profile (an installed app, a development build, a review run) has its
/// own running app.
fn profile_key(profile_root: &Path) -> String {
    let text = profile_root.to_string_lossy();
    // Windows paths ignore case.
    let text = if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.into_owned()
    };
    format!("{:016x}", fnv1a(text.as_bytes()))
}

impl Endpoint {
    /// The channel of the profile whose files are under `profile_root`, or
    /// `None` where there is no private place for it.
    pub fn for_profile(profile_root: &Path) -> Option<Self> {
        let name = format!("butter-paper-open-{}", profile_key(profile_root));
        #[cfg(unix)]
        {
            // Linux: the user's runtime folder. macOS: the per-user
            // temporary folder.
            let folder = if cfg!(target_os = "macos") {
                Some(std::env::temp_dir())
            } else {
                std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
            }?;
            Self::in_folder(&folder, &name)
        }
        #[cfg(windows)]
        {
            let sid = windows::current_user_sid()?;
            Some(Self {
                pipe: pipe_name(&sid, &name),
                sid,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = name;
            None
        }
    }

    /// A socket named `name` in `folder`, or `None` unless the folder is a
    /// real folder (not a link) that belongs to the user and that no one
    /// else may enter or list.
    #[cfg(unix)]
    pub fn in_folder(folder: &Path, name: &str) -> Option<Self> {
        use std::os::unix::fs::MetadataExt as _;
        if !folder.is_absolute() {
            return None;
        }
        let metadata = std::fs::symlink_metadata(folder).ok()?;
        if !metadata.is_dir() || !folder_is_private(metadata.uid(), metadata.mode(), current_uid())
        {
            return None;
        }
        let socket = folder.join(format!("{name}.sock"));
        // A socket path is limited to about 104 bytes.
        (socket.as_os_str().len() < 100).then_some(Self { socket })
    }

    /// An endpoint at a given socket path, for tests.
    #[cfg(unix)]
    pub fn at_socket(socket: PathBuf) -> Self {
        Self { socket }
    }

    /// An endpoint at a given pipe name for the current user, for tests.
    #[cfg(windows)]
    pub fn at_pipe(pipe: String) -> Self {
        Self {
            pipe,
            sid: windows::current_user_sid().expect("the current user's SID"),
        }
    }
}

/// Whether a folder owned by `owner` with permission bits `mode` keeps a
/// socket private to the user `me`: the user's own, with nothing for the
/// group or others.
#[cfg_attr(not(unix), allow(dead_code))]
fn folder_is_private(owner: u32, mode: u32, me: u32) -> bool {
    owner == me && mode & 0o077 == 0
}

/// Whether the other end of a connection may be trusted: it runs as this
/// launch's user. `peer` is `None` when the system could not say.
#[cfg_attr(not(any(unix, windows)), allow(dead_code))]
fn peer_is_trusted<T: PartialEq>(peer: Option<T>, me: &T) -> bool {
    peer.is_some_and(|peer| peer == *me)
}

/// The pipe's name: the profile's channel name and the user's SID, so each
/// user and profile has its own. (A pipe name may hold no backslash after
/// `\\.\pipe\`.)
#[cfg_attr(not(windows), allow(dead_code))]
fn pipe_name(sid: &str, name: &str) -> String {
    format!(r"\\.\pipe\{name}-{sid}")
}

/// The pipe's security descriptor: a protected access list that gives the
/// user (`sid`) and SYSTEM full access, and no one else any.
#[cfg_attr(not(windows), allow(dead_code))]
fn pipe_security(sid: &str) -> String {
    format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)")
}

#[cfg(unix)]
fn current_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

/// The user id of the process at the other end of `stream`.
#[cfg(target_os = "linux")]
fn peer_uid(stream: &std::os::unix::net::UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd as _;
    let mut credentials = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: the buffer is a ucred and the length says so.
    let result = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    (result == 0).then_some(credentials.uid)
}

/// The user id of the process at the other end of `stream`.
#[cfg(all(unix, not(target_os = "linux")))]
fn peer_uid(stream: &std::os::unix::net::UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd as _;
    let mut uid = 0;
    let mut gid = 0;
    // SAFETY: both pointers are to live integers of the kinds asked for.
    let result = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    (result == 0).then_some(uid)
}

/// A request carrying `paths`; `None` when a path cannot be carried (not
/// absolute, not valid Unicode on Windows, or holding a line break).
pub fn encode_request(paths: &[PathBuf]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    for path in paths {
        if !path.is_absolute() {
            return None;
        }
        let bytes = path_bytes(path)?;
        if bytes.contains(&b'\n') || bytes.contains(&0) {
            return None;
        }
        body.extend_from_slice(&bytes);
        body.push(b'\n');
    }
    let mut request = Vec::with_capacity(4 + MAGIC.len() + body.len());
    request.extend_from_slice(&((MAGIC.len() + body.len()) as u32).to_le_bytes());
    request.extend_from_slice(MAGIC);
    request.extend_from_slice(&body);
    (request.len() <= MAX_REQUEST).then_some(request)
}

/// The PDFs a request's body (after its length) names: absolute paths that
/// end in `.pdf`. Anything else in it is ignored; a body without the
/// protocol's first line names none.
pub fn decode_request(body: &[u8]) -> Vec<PathBuf> {
    let Some(lines) = body.strip_prefix(MAGIC) else {
        return Vec::new();
    };
    lines
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(path_from_bytes)
        .filter(|path| {
            path.is_absolute()
                && path.extension().is_some_and(|extension| {
                    extension.as_encoded_bytes().eq_ignore_ascii_case(b"pdf")
                })
        })
        .collect()
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> Option<Vec<u8>> {
    use std::os::unix::ffi::OsStrExt as _;
    Some(path.as_os_str().as_bytes().to_vec())
}

#[cfg(not(unix))]
fn path_bytes(path: &Path) -> Option<Vec<u8>> {
    path.to_str().map(|text| text.as_bytes().to_vec())
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt as _;
    Some(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> Option<PathBuf> {
    std::str::from_utf8(bytes).ok().map(PathBuf::from)
}

/// Offers `paths` to a running app of the profile, or claims the channel.
/// A launch with no PDFs never forwards: it runs as the app always has.
pub fn claim(profile_root: &Path, paths: &[PathBuf]) -> InstanceRole {
    match Endpoint::for_profile(profile_root) {
        Some(endpoint) => claim_at(&endpoint, paths),
        None => InstanceRole::Primary(None),
    }
}

/// [`claim`] at a given endpoint.
pub fn claim_at(endpoint: &Endpoint, paths: &[PathBuf]) -> InstanceRole {
    if !paths.is_empty()
        && let Some(request) = encode_request(paths)
        && forward(endpoint, request)
    {
        return InstanceRole::Forwarded;
    }
    InstanceRole::Primary(listen(endpoint))
}

/// Sends a request and waits for the running app's answer, on a thread of
/// its own so a running app that never answers cannot hold the launch.
fn forward(endpoint: &Endpoint, request: Vec<u8>) -> bool {
    let endpoint = endpoint.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(send_request(&endpoint, &request).unwrap_or(false));
    });
    receiver.recv_timeout(FORWARD_TIMEOUT).unwrap_or(false)
}

impl InstanceListener {
    /// Hands each later launch's PDFs to `on_paths`, on a thread of its
    /// own, for as long as the app runs. Call [`ChannelCleanup::remove`] as
    /// the app quits: the thread runs until the process ends, so nothing
    /// else removes a socket file.
    pub fn serve(self, on_paths: impl Fn(Vec<PathBuf>) + Send + 'static) -> ChannelCleanup {
        let cleanup = ChannelCleanup {
            #[cfg(unix)]
            socket: Some(self.socket.clone()),
        };
        std::thread::Builder::new()
            .name("butter-paper-open-listener".into())
            .spawn(move || self.run(on_paths))
            .ok();
        cleanup
    }
}

/// Removes the channel's socket file when the app quits. A socket left by
/// an app that stopped without quitting is replaced at the next launch.
pub struct ChannelCleanup {
    #[cfg(unix)]
    socket: Option<PathBuf>,
}

impl ChannelCleanup {
    pub fn remove(&mut self) {
        #[cfg(unix)]
        if let Some(socket) = self.socket.take() {
            let _ = std::fs::remove_file(socket);
        }
    }
}

#[cfg(unix)]
fn send_request(endpoint: &Endpoint, request: &[u8]) -> std::io::Result<bool> {
    use std::io::{Read as _, Write as _};
    let mut stream = std::os::unix::net::UnixStream::connect(&endpoint.socket)?;
    // Only a listener of this launch's own user gets its PDFs.
    if !peer_is_trusted(peer_uid(&stream), &current_uid()) {
        return Ok(false);
    }
    stream.set_read_timeout(Some(FORWARD_TIMEOUT))?;
    stream.set_write_timeout(Some(FORWARD_TIMEOUT))?;
    stream.write_all(request)?;
    let mut reply = [0; REPLY.len()];
    stream.read_exact(&mut reply)?;
    Ok(reply == REPLY)
}

#[cfg(unix)]
fn listen(endpoint: &Endpoint) -> Option<InstanceListener> {
    use std::os::unix::fs::PermissionsExt as _;
    use std::os::unix::net::{UnixListener, UnixStream};
    let bind = || UnixListener::bind(&endpoint.socket);
    let listener = match bind() {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            // A running app answers there: leave it the channel. Otherwise
            // the socket is left from an app that stopped.
            if UnixStream::connect(&endpoint.socket).is_ok() {
                return None;
            }
            std::fs::remove_file(&endpoint.socket).ok()?;
            bind().ok()?
        }
        Err(_) => return None,
    };
    // The folder is the user's alone, so no one else can reach the socket
    // before its own permissions are limited too.
    std::fs::set_permissions(&endpoint.socket, std::fs::Permissions::from_mode(0o600)).ok();
    Some(InstanceListener {
        listener,
        socket: endpoint.socket.clone(),
    })
}

#[cfg(unix)]
impl InstanceListener {
    fn run(self, on_paths: impl Fn(Vec<PathBuf>)) {
        let me = current_uid();
        for stream in self.listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            // A peer of another user is dropped unread.
            if !peer_is_trusted(peer_uid(&stream), &me) {
                continue;
            }
            if let Some(paths) = read_request(&mut stream) {
                use std::io::Write as _;
                let _ = stream.write_all(REPLY);
                if !paths.is_empty() {
                    on_paths(paths);
                }
            }
        }
    }
}

#[cfg(unix)]
impl Drop for InstanceListener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}

#[cfg(unix)]
fn read_request(stream: &mut std::os::unix::net::UnixStream) -> Option<Vec<PathBuf>> {
    use std::io::Read as _;
    stream.set_read_timeout(Some(FORWARD_TIMEOUT)).ok()?;
    let mut length = [0; 4];
    stream.read_exact(&mut length).ok()?;
    let length = u32::from_le_bytes(length) as usize;
    if length > MAX_REQUEST {
        return None;
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body).ok()?;
    Some(decode_request(&body))
}

#[cfg(windows)]
fn send_request(endpoint: &Endpoint, request: &[u8]) -> std::io::Result<bool> {
    windows::send_request(&endpoint.pipe, &endpoint.sid, request)
}

#[cfg(windows)]
fn listen(endpoint: &Endpoint) -> Option<InstanceListener> {
    let first = windows::PipeHandle::create(&endpoint.pipe, &endpoint.sid, true)?;
    Some(InstanceListener {
        first,
        pipe: endpoint.pipe.clone(),
        sid: endpoint.sid.clone(),
    })
}

#[cfg(windows)]
impl InstanceListener {
    fn run(self, on_paths: impl Fn(Vec<PathBuf>)) {
        let mut current = self.first;
        loop {
            if !current.wait_for_client() {
                return;
            }
            // The next instance exists before this one closes, so a launch
            // never finds the channel missing.
            let next = windows::PipeHandle::create(&self.pipe, &self.sid, false);
            // A launch that sends nothing, or never reads the reply, must
            // not block the listener: its I/O is cancelled at the deadline.
            let paths = {
                let _deadline = windows::IoDeadline::arm(SERVE_TIMEOUT);
                current.read_request().inspect(|_| {
                    current.write_all(REPLY);
                })
            };
            if let Some(paths) = paths
                && !paths.is_empty()
            {
                on_paths(paths);
            }
            current.disconnect();
            match next {
                Some(next) => current = next,
                None => return,
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn send_request(_: &Endpoint, _: &[u8]) -> std::io::Result<bool> {
    Ok(false)
}

#[cfg(not(any(unix, windows)))]
fn listen(_: &Endpoint) -> Option<InstanceListener> {
    None
}

#[cfg(not(any(unix, windows)))]
impl InstanceListener {
    fn run(self, _: impl Fn(Vec<PathBuf>)) {}
}

#[cfg(windows)]
mod windows {
    //! The named pipe, through the Win32 API.

    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GENERIC_READ, GENERIC_WRITE,
        GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
        TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, OPEN_EXISTING,
        PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
    };
    use windows_sys::Win32::System::IO::CancelSynchronousIo;
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId,
        PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES,
        PIPE_WAIT, WaitNamedPipeW,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThreadId, OpenProcess, OpenProcessToken, OpenThread,
        PROCESS_QUERY_LIMITED_INFORMATION, THREAD_TERMINATE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};

    use super::{
        FORWARD_TIMEOUT, MAX_REQUEST, REPLY, decode_request, peer_is_trusted, pipe_security,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    /// An owned process or token handle, closed when dropped.
    struct OwnedHandle(HANDLE);

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            // SAFETY: the handle is owned and open.
            unsafe { CloseHandle(self.0) };
        }
    }

    // SAFETY: a thread or process handle is a kernel object handle, usable
    // from any thread.
    unsafe impl Send for OwnedHandle {}

    /// Cancels the blocking I/O of the thread that armed it when `timeout`
    /// passes before it is dropped. Dropping it waits for its watchdog.
    pub struct IoDeadline {
        disarm: Option<std::sync::mpsc::Sender<()>>,
        watchdog: Option<std::thread::JoinHandle<()>>,
    }

    impl IoDeadline {
        pub fn arm(timeout: std::time::Duration) -> Self {
            // SAFETY: opens this thread for CancelSynchronousIo only; the
            // OwnedHandle closes it.
            let thread = unsafe { OpenThread(THREAD_TERMINATE, 0, GetCurrentThreadId()) };
            if thread.is_null() {
                return Self {
                    disarm: None,
                    watchdog: None,
                };
            }
            let thread = OwnedHandle(thread);
            let (disarm, disarmed) = std::sync::mpsc::channel::<()>();
            let watchdog = std::thread::Builder::new()
                .name("butter-paper-open-deadline".into())
                .spawn(move || {
                    // The whole handle moves here, not only its raw field.
                    let thread = thread;
                    if disarmed.recv_timeout(timeout)
                        == Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                    {
                        // SAFETY: the handle is open with THREAD_TERMINATE
                        // access. With no I/O pending this does nothing.
                        unsafe { CancelSynchronousIo(thread.0) };
                    }
                })
                .ok();
            Self {
                disarm: Some(disarm),
                watchdog,
            }
        }
    }

    impl Drop for IoDeadline {
        fn drop(&mut self) {
            drop(self.disarm.take());
            if let Some(watchdog) = self.watchdog.take() {
                let _ = watchdog.join();
            }
        }
    }

    /// Connects to the pipe and sends nothing, as a stalled launch can.
    #[cfg(test)]
    pub fn connect_silently(name: &str) -> Option<PipeHandle> {
        let name = wide(name);
        // SAFETY: the name is NUL-terminated and outlives the call.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        (handle != INVALID_HANDLE_VALUE).then_some(PipeHandle(handle))
    }

    /// The SID, as text, of the user `process` runs as.
    fn process_user_sid(process: HANDLE) -> Option<String> {
        let mut token = std::ptr::null_mut();
        // SAFETY: `process` is a live process handle; `token` receives a
        // handle the OwnedHandle closes.
        if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        let token = OwnedHandle(token);
        let mut length = 0;
        // SAFETY: asks for the size only, with no buffer.
        unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut length) };
        if length == 0 {
            return None;
        }
        // u64s, so the TOKEN_USER in it is aligned.
        let mut buffer = vec![0_u64; (length as usize).div_ceil(8)];
        // SAFETY: the buffer holds at least `length` bytes.
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } == 0
        {
            return None;
        }
        // SAFETY: GetTokenInformation wrote a TOKEN_USER at the buffer's
        // start, whose SID points into the same buffer.
        let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
        let mut text = std::ptr::null_mut();
        // SAFETY: `sid` is valid while the buffer lives; `text` receives a
        // string that LocalFree releases.
        if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
            return None;
        }
        // SAFETY: `text` is a NUL-terminated UTF-16 string.
        let length = (0..)
            .take_while(|index| unsafe { *text.add(*index) } != 0)
            .count();
        // SAFETY: `length` units precede the NUL.
        let sid = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) }).ok();
        // SAFETY: allocated by ConvertSidToStringSidW.
        unsafe { LocalFree(text.cast()) };
        sid
    }

    /// The SID, as text, of the user this process runs as.
    pub fn current_user_sid() -> Option<String> {
        // SAFETY: the pseudo handle of the current process needs no closing.
        process_user_sid(unsafe { GetCurrentProcess() })
    }

    /// The SID, as text, of the user the server of a connected pipe runs
    /// as; `None` when Windows will not say (another user's process).
    fn pipe_server_sid(pipe: HANDLE) -> Option<String> {
        let mut process_id = 0;
        // SAFETY: `pipe` is a connected client end.
        if unsafe { GetNamedPipeServerProcessId(pipe, &mut process_id) } == 0 {
            return None;
        }
        // SAFETY: opens for limited queries only; the handle is closed.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if process.is_null() {
            return None;
        }
        let process = OwnedHandle(process);
        process_user_sid(process.0)
    }

    /// An owned pipe handle, closed when dropped.
    pub struct PipeHandle(HANDLE);

    // SAFETY: a pipe handle is a kernel object handle, usable from any thread.
    unsafe impl Send for PipeHandle {}

    impl Drop for PipeHandle {
        fn drop(&mut self) {
            // SAFETY: the handle is owned and open.
            unsafe { CloseHandle(self.0) };
        }
    }

    impl PipeHandle {
        /// An instance of the pipe to listen on, open only to the user
        /// `sid` and SYSTEM; `first` fails when the pipe already exists, so
        /// only one app listens.
        pub fn create(name: &str, sid: &str, first: bool) -> Option<Self> {
            let name = wide(name);
            let sddl = wide(&pipe_security(sid));
            let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            // SAFETY: the SDDL is NUL-terminated; `descriptor` receives
            // memory that LocalFree releases below.
            if unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return None;
            }
            let attributes = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            };
            let open_mode = PIPE_ACCESS_DUPLEX
                | if first {
                    FILE_FLAG_FIRST_PIPE_INSTANCE
                } else {
                    0
                };
            // SAFETY: the name is NUL-terminated and outlives the call, as
            // do the attributes and their descriptor.
            let handle = unsafe {
                CreateNamedPipeW(
                    name.as_ptr(),
                    open_mode,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_UNLIMITED_INSTANCES,
                    4096,
                    4096,
                    0,
                    &attributes,
                )
            };
            // SAFETY: allocated by the conversion above; the pipe keeps its
            // own copy.
            unsafe { LocalFree(descriptor) };
            (handle != INVALID_HANDLE_VALUE).then_some(Self(handle))
        }

        /// Waits until a launch connects.
        pub fn wait_for_client(&self) -> bool {
            // SAFETY: the handle is an open pipe instance; no overlapped I/O.
            let connected = unsafe { ConnectNamedPipe(self.0, std::ptr::null_mut()) } != 0;
            // SAFETY: reads the calling thread's last error.
            connected || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED
        }

        fn read_exact(&self, buffer: &mut [u8]) -> bool {
            let mut filled = 0;
            while filled < buffer.len() {
                let mut read = 0;
                // SAFETY: the buffer range is valid for the length given.
                let ok = unsafe {
                    ReadFile(
                        self.0,
                        buffer[filled..].as_mut_ptr(),
                        (buffer.len() - filled) as u32,
                        &mut read,
                        std::ptr::null_mut(),
                    )
                } != 0;
                if !ok || read == 0 {
                    return false;
                }
                filled += read as usize;
            }
            true
        }

        pub fn read_request(&self) -> Option<Vec<PathBuf>> {
            let mut length = [0; 4];
            if !self.read_exact(&mut length) {
                return None;
            }
            let length = u32::from_le_bytes(length) as usize;
            if length > MAX_REQUEST {
                return None;
            }
            let mut body = vec![0; length];
            self.read_exact(&mut body).then(|| decode_request(&body))
        }

        pub fn write_all(&self, bytes: &[u8]) -> bool {
            let mut written = 0;
            // SAFETY: the bytes are valid for the length given.
            let ok = unsafe {
                WriteFile(
                    self.0,
                    bytes.as_ptr(),
                    bytes.len() as u32,
                    &mut written,
                    std::ptr::null_mut(),
                )
            } != 0;
            // SAFETY: the handle is open; this waits until the client read.
            ok && written as usize == bytes.len() && unsafe { FlushFileBuffers(self.0) } != 0
        }

        pub fn disconnect(&self) {
            // SAFETY: the handle is an open, connected pipe instance.
            unsafe { DisconnectNamedPipe(self.0) };
        }
    }

    /// Connects to the running app's pipe, sends `request` and reads its
    /// answer. Nothing is sent unless the pipe's server runs as `sid`, this
    /// launch's user. The running app may then bring its window to the
    /// front.
    pub fn send_request(name: &str, sid: &str, request: &[u8]) -> std::io::Result<bool> {
        let wide_name = wide(name);
        let handle = loop {
            // SAFETY: the name is NUL-terminated and outlives the call.
            let handle = unsafe {
                CreateFileW(
                    wide_name.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
            if handle != INVALID_HANDLE_VALUE {
                break PipeHandle(handle);
            }
            // SAFETY: reads the calling thread's last error.
            if unsafe { GetLastError() } != ERROR_PIPE_BUSY
                // SAFETY: the name is NUL-terminated.
                || unsafe { WaitNamedPipeW(wide_name.as_ptr(), FORWARD_TIMEOUT.as_millis() as u32) }
                    == 0
            {
                return Ok(false);
            }
        };
        // A pipe another user made first gets nothing.
        if !peer_is_trusted(pipe_server_sid(handle.0), &sid.to_owned()) {
            return Ok(false);
        }
        // This launch has the foreground the file manager gave it; let the
        // running app take it.
        // SAFETY: no pointers; failure only means Windows keeps the focus.
        unsafe { AllowSetForegroundWindow(ASFW_ANY) };
        if !handle.write_all(request) {
            return Ok(false);
        }
        let mut reply = [0; REPLY.len()];
        Ok(handle.read_exact(&mut reply) && reply == REPLY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pdf(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    #[test]
    fn a_request_carries_absolute_pdf_paths_in_order() {
        let paths = [pdf("b.pdf"), pdf("a file.PDF")];
        let request = encode_request(&paths).unwrap();
        let length = u32::from_le_bytes(request[..4].try_into().unwrap()) as usize;
        assert_eq!(length, request.len() - 4);
        assert_eq!(decode_request(&request[4..]), paths);
        // Not absolute, or with a line break: not carried.
        assert_eq!(encode_request(&[PathBuf::from("a.pdf")]), None);
        assert_eq!(encode_request(&[pdf("a\nb.pdf")]), None);
    }

    #[test]
    fn a_request_names_only_absolute_pdfs_after_its_first_line() {
        let mut body = MAGIC.to_vec();
        body.extend_from_slice(b"relative.pdf\n");
        body.extend_from_slice(pdf("notes.txt").to_string_lossy().as_bytes());
        body.push(b'\n');
        body.extend_from_slice(pdf("kept.pdf").to_string_lossy().as_bytes());
        body.push(b'\n');
        assert_eq!(decode_request(&body), [pdf("kept.pdf")]);
        assert!(decode_request(b"GET / HTTP/1.1\n/x.pdf\n").is_empty());
    }

    #[test]
    fn each_profile_has_its_own_channel() {
        let a = profile_key(Path::new("/home/a/.local/share/butter-paper"));
        let b = profile_key(Path::new("/home/a/dev-profile"));
        assert_ne!(a, b);
        assert_eq!(
            a,
            profile_key(Path::new("/home/a/.local/share/butter-paper"))
        );
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn a_socket_folder_must_be_the_users_alone() {
        assert!(folder_is_private(501, 0o40700, 501));
        assert!(!folder_is_private(501, 0o40750, 501), "the group may enter");
        assert!(!folder_is_private(501, 0o41777, 501), "anyone may write");
        assert!(!folder_is_private(0, 0o40700, 501), "another user's");
    }

    #[test]
    fn only_a_peer_of_the_same_user_is_trusted() {
        assert!(peer_is_trusted(Some(501_u32), &501));
        assert!(
            !peer_is_trusted(Some(0_u32), &501),
            "another user's process"
        );
        assert!(!peer_is_trusted(None::<u32>, &501), "unknown");
        let me = "S-1-5-21-1-2-3-1001".to_owned();
        assert!(peer_is_trusted(Some(me.clone()), &me));
        assert!(!peer_is_trusted(
            Some("S-1-5-21-1-2-3-1002".to_owned()),
            &me
        ));
    }

    #[test]
    fn the_pipe_is_named_for_and_open_only_to_its_user() {
        let sid = "S-1-5-21-1-2-3-1001";
        let name = pipe_name(sid, "butter-paper-open-0123456789abcdef");
        assert_eq!(
            name,
            r"\\.\pipe\butter-paper-open-0123456789abcdef-S-1-5-21-1-2-3-1001"
        );
        assert_eq!(
            pipe_security(sid),
            "D:P(A;;GA;;;S-1-5-21-1-2-3-1001)(A;;GA;;;SY)"
        );
    }

    /// A socket folder that others may enter is not used.
    #[cfg(unix)]
    #[test]
    fn a_shared_folder_gets_no_socket() {
        use std::os::unix::fs::PermissionsExt as _;
        let folder = std::env::temp_dir().join(format!("bp-si-folder-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(Endpoint::in_folder(&folder, "open").is_some());
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Endpoint::in_folder(&folder, "open").is_none());
        let _ = std::fs::remove_dir_all(folder);
    }

    /// On Windows the pipe's name carries the user's SID.
    #[cfg(windows)]
    #[test]
    fn the_profile_pipe_names_the_current_user() {
        let sid = windows::current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-"), "{sid}");
        let endpoint = Endpoint::for_profile(Path::new(r"C:\profile")).unwrap();
        assert!(endpoint.pipe.contains(&sid), "{}", endpoint.pipe);
    }

    /// A second launch with PDFs hands them to the first and exits; the
    /// first opens them. A launch without PDFs runs as an app of its own.
    #[cfg(unix)]
    #[test]
    fn a_second_launch_hands_its_pdfs_to_the_running_app() {
        let folder = std::env::temp_dir().join(format!("bp-si-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let endpoint = Endpoint::at_socket(folder.join("open.sock"));
        let InstanceRole::Primary(Some(listener)) = claim_at(&endpoint, &[pdf("first.pdf")]) else {
            panic!("the first launch listens");
        };
        let (sender, received) = std::sync::mpsc::channel();
        listener.serve(move |paths| sender.send(paths).unwrap());
        let paths = vec![pdf("second.pdf"), pdf("third.pdf")];
        assert!(matches!(
            claim_at(&endpoint, &paths),
            InstanceRole::Forwarded
        ));
        assert_eq!(received.recv_timeout(FORWARD_TIMEOUT).unwrap(), paths);
        assert!(matches!(
            claim_at(&endpoint, &[]),
            InstanceRole::Primary(None)
        ));
        let _ = std::fs::remove_dir_all(folder);
    }

    /// The same on Windows, through the named pipe: a second launch hands
    /// over its PDFs, twice in a row, and a launch without PDFs runs alone.
    #[cfg(windows)]
    #[test]
    fn a_second_launch_hands_its_pdfs_to_the_running_app_through_the_pipe() {
        let sid = windows::current_user_sid().unwrap();
        let endpoint = Endpoint::at_pipe(pipe_name(
            &sid,
            &format!("butter-paper-open-test-{}", std::process::id()),
        ));
        let InstanceRole::Primary(Some(listener)) = claim_at(&endpoint, &[pdf("first.pdf")]) else {
            panic!("the first launch listens");
        };
        let (sender, received) = std::sync::mpsc::channel();
        let _cleanup = listener.serve(move |paths| sender.send(paths).unwrap());
        for name in ["second.pdf", "third.pdf"] {
            let paths = vec![pdf(name)];
            assert!(matches!(
                claim_at(&endpoint, &paths),
                InstanceRole::Forwarded
            ));
            assert_eq!(received.recv_timeout(FORWARD_TIMEOUT).unwrap(), paths);
        }
        assert!(matches!(
            claim_at(&endpoint, &[]),
            InstanceRole::Primary(None)
        ));
    }

    /// A launch that connects and sends nothing does not stop the next
    /// launch from handing over its PDFs.
    #[cfg(windows)]
    #[test]
    fn a_silent_client_does_not_block_the_pipe() {
        let sid = windows::current_user_sid().unwrap();
        let endpoint = Endpoint::at_pipe(pipe_name(
            &sid,
            &format!("butter-paper-open-silent-{}", std::process::id()),
        ));
        let InstanceRole::Primary(Some(listener)) = claim_at(&endpoint, &[pdf("first.pdf")]) else {
            panic!("the first launch listens");
        };
        let (sender, received) = std::sync::mpsc::channel();
        let _cleanup = listener.serve(move |paths| sender.send(paths).unwrap());
        let silent = windows::connect_silently(&endpoint.pipe).expect("the pipe accepts");
        let paths = vec![pdf("after-silent.pdf")];
        assert!(matches!(
            claim_at(&endpoint, &paths),
            InstanceRole::Forwarded
        ));
        assert_eq!(received.recv_timeout(FORWARD_TIMEOUT).unwrap(), paths);
        drop(silent);
    }

    /// A socket left by an app that stopped does not stop the next launch
    /// from listening.
    #[cfg(unix)]
    #[test]
    fn a_stale_socket_is_replaced() {
        let folder = std::env::temp_dir().join(format!("bp-si-stale-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let socket = folder.join("open.sock");
        drop(std::os::unix::net::UnixListener::bind(&socket).unwrap());
        assert!(socket.exists());
        let endpoint = Endpoint::at_socket(socket);
        assert!(matches!(
            claim_at(&endpoint, &[pdf("a.pdf")]),
            InstanceRole::Primary(Some(_))
        ));
        let _ = std::fs::remove_dir_all(folder);
    }
}
