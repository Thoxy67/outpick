//! Ported from Luma (MIT, Copyright (c) 2026 Luma contributors): lg-luma/src/luna.rs,
//! without the app-impersonating subscription.
use libc::{c_char, c_int, c_uint, c_ulong, c_void};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    ffi::{CStr, CString},
    panic::{AssertUnwindSafe, catch_unwind},
    ptr,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const MAX_REPLY: usize = 2 * 1024 * 1024;

// Layout from the NDK's public luna-service2/lunaservice.h.
#[repr(C)]
struct Error {
    code: c_int,
    message: *mut c_char,
    file: *const c_char,
    line: c_int,
    function: *const c_char,
    padding: *mut c_void,
    magic: c_ulong,
}

#[link(name = "luna-service2")]
unsafe extern "C" {
    fn LSErrorInit(error: *mut Error) -> bool;
    fn LSErrorFree(error: *mut Error);
    fn LSRegister(name: *const c_char, handle: *mut *mut c_void, error: *mut Error) -> bool;
    fn LSUnregister(handle: *mut c_void, error: *mut Error) -> bool;
    fn LSGmainContextAttach(handle: *mut c_void, context: *mut c_void, error: *mut Error) -> bool;
    fn LSCallOneReply(
        handle: *mut c_void,
        uri: *const c_char,
        payload: *const c_char,
        callback: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool,
        userdata: *mut c_void,
        token: *mut c_ulong,
        error: *mut Error,
    ) -> bool;
    fn LSCall(
        handle: *mut c_void,
        uri: *const c_char,
        payload: *const c_char,
        callback: extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> bool,
        userdata: *mut c_void,
        token: *mut c_ulong,
        error: *mut Error,
    ) -> bool;
    fn LSCallCancel(handle: *mut c_void, token: c_ulong, error: *mut Error) -> bool;
    fn LSMessageGetPayload(message: *mut c_void) -> *const c_char;
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_main_context_new() -> *mut c_void;
    fn g_main_context_iteration(context: *mut c_void, may_block: c_int) -> c_int;
    fn g_main_context_unref(context: *mut c_void);
    fn g_timeout_source_new(interval: c_uint) -> *mut c_void;
    fn g_source_set_callback(
        source: *mut c_void,
        func: extern "C" fn(*mut c_void) -> c_int,
        data: *mut c_void,
        notify: *const c_void,
    );
    fn g_source_attach(source: *mut c_void, context: *mut c_void) -> c_uint;
    fn g_source_destroy(source: *mut c_void);
    fn g_source_unref(source: *mut c_void);
}

/// The request deadline: marks it passed and removes its source (G_SOURCE_REMOVE).
extern "C" fn deadline_passed(flag: *mut c_void) -> c_int {
    // SAFETY: `flag` is the Cell on `request`'s stack, which destroys this source before returning.
    unsafe { (*flag.cast::<Cell<bool>>()).set(true) };
    0
}

impl Error {
    fn new() -> Self {
        // SAFETY: the public error structure is zero-initializable; LS2 then
        // initializes its cookie before any other error API uses it.
        unsafe {
            let mut error = std::mem::zeroed();
            LSErrorInit(&mut error);
            error
        }
    }

    fn describe(&self, stage: &str) -> String {
        let detail = if self.message.is_null() {
            String::new()
        } else {
            // SAFETY: LS2 owns this NUL-terminated string until LSErrorFree.
            unsafe { CStr::from_ptr(self.message) }
                .to_string_lossy()
                .chars()
                .take(240)
                .collect()
        };
        format!("LS2 {stage} failed ({}): {detail}", self.code)
    }
}

impl Drop for Error {
    fn drop(&mut self) {
        // SAFETY: every Error was initialized and is freed exactly once.
        unsafe { LSErrorFree(self) };
    }
}

#[derive(Default)]
struct Reply {
    result: Option<Result<Value, String>>,
}

extern "C" fn receive(_handle: *mut c_void, message: *mut c_void, userdata: *mut c_void) -> bool {
    if message.is_null() || userdata.is_null() {
        return true;
    }
    // SAFETY: LS2 invokes this only while this connection's context is iterated
    // on the worker. The heap slot outlives cancellation/unregistration.
    unsafe {
        let reply = &mut *userdata.cast::<Reply>();
        let payload = LSMessageGetPayload(message);
        reply.result = Some(if payload.is_null() {
            Err("Luna reply has no payload".into())
        } else {
            let size = libc::strnlen(payload, MAX_REPLY + 1);
            if size > MAX_REPLY {
                Err("TV response is too large".into())
            } else {
                let bytes = std::slice::from_raw_parts(payload.cast::<u8>(), size);
                serde_json::from_slice(bytes).map_err(|error| format!("Invalid Luna JSON: {error}"))
            }
        });
    }
    true
}

struct Connection {
    handle: *mut c_void,
    context: *mut c_void,
    reply: Option<Box<Reply>>,
}

impl Connection {
    fn open() -> Result<Self, String> {
        Self::open_named(None)
    }

    /// `name`: a service name the executable's LS2 role allows (sam identifies a registerApp
    /// caller by it); None registers anonymously.
    fn open_named(name: Option<&CStr>) -> Result<Self, String> {
        let mut error = Error::new();
        let mut connection = Self {
            handle: ptr::null_mut(),
            // SAFETY: GLib returns a new context owned by this connection.
            context: unsafe { g_main_context_new() },
            reply: None,
        };
        if connection.context.is_null() {
            return Err("Could not allocate a Luna event context".into());
        }
        // Anonymous registration still authenticates the executable and its LS2
        // role, and avoids colliding with SDL's own app-named registration.
        // SAFETY: output/error pointers are valid; no fake application ID is sent.
        let name = name.map_or(ptr::null(), CStr::as_ptr);
        if !unsafe { LSRegister(name, &mut connection.handle, &mut error) } {
            return Err(error.describe("registration"));
        }
        // SAFETY: the registered handle and private context stay alive together.
        if !unsafe { LSGmainContextAttach(connection.handle, connection.context, &mut error) } {
            return Err(error.describe("context attachment"));
        }
        Ok(connection)
    }

    fn request(&mut self, uri: &str, payload: &Value, timeout: Duration) -> Result<Value, String> {
        let uri = CString::new(uri).map_err(|_| "Invalid Luna URI")?;
        let payload = CString::new(payload.to_string()).map_err(|_| "Invalid Luna payload")?;
        let mut error = Error::new();
        let mut reply = Box::<Reply>::default();
        let userdata = ptr::from_mut(reply.as_mut()).cast();
        self.reply = Some(reply);
        let mut token = 0;
        // SAFETY: strings and the stable reply allocation outlive this request.
        if !unsafe {
            LSCallOneReply(
                self.handle,
                uri.as_ptr(),
                payload.as_ptr(),
                receive,
                userdata,
                &mut token,
                &mut error,
            )
        } {
            return Err(error.describe(SUBMISSION));
        }
        // Block on the private context until the reply or the deadline (a GLib
        // timeout source on the same context), instead of polling.
        let timed_out = Cell::new(false);
        let millis = timeout.as_millis().clamp(1, c_uint::MAX as u128) as c_uint;
        // SAFETY: the source is attached to this connection's private context and
        // destroyed (then unreferenced) below, before `timed_out` goes out of scope.
        let source = unsafe {
            let source = g_timeout_source_new(millis);
            g_source_set_callback(source, deadline_passed, ptr::from_ref(&timed_out).cast_mut().cast(), ptr::null());
            g_source_attach(source, self.context);
            source
        };
        let deadline = Instant::now() + timeout;
        let result = loop {
            // SAFETY: only this thread drives this private context. The timeout
            // source bounds the wait, so a missing service cannot hang the caller.
            unsafe { g_main_context_iteration(self.context, 1) };
            if let Some(result) = self.reply.as_mut().and_then(|reply| reply.result.take()) {
                break Some(result);
            }
            if timed_out.get() || Instant::now() >= deadline + Duration::from_millis(50) {
                break None;
            }
        };
        // SAFETY: destroying an already removed source is a no-op; the unref drops our reference.
        unsafe {
            g_source_destroy(source);
            g_source_unref(source);
        }
        if let Some(result) = result {
            return result;
        }
        let mut cancel_error = Error::new();
        // SAFETY: handle/token belong to this live request. Even a refused cancel
        // leaves its callback allocation alive until unregistering on drop.
        if !unsafe { LSCallCancel(self.handle, token, &mut cancel_error) } {
            eprintln!("{}", cancel_error.describe("timeout cancellation"));
        }
        Err(TIMED_OUT.into())
    }
}

const TIMED_OUT: &str = "The TV service timed out";
const SUBMISSION: &str = "call submission";
const SUBMISSION_FAILED: &str = "LS2 call submission";

impl Drop for Connection {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            let mut error = Error::new();
            // SAFETY: unregister before freeing the context or callback storage.
            if !unsafe { LSUnregister(self.handle, &mut error) } {
                eprintln!("{}", error.describe("unregistration"));
                // A refused unregister can retain the callback. Preserve its
                // context and heap slot rather than leave dangling pointers.
                std::mem::forget(self.reply.take());
                return;
            }
        }
        if !self.context.is_null() {
            // SAFETY: no live LS2 connection can now dispatch on this context.
            unsafe { g_main_context_unref(self.context) };
        }
    }
}

pub fn call(uri: &str, payload: &Value) -> Result<Value, String> {
    call_with_timeout(uri, payload, Duration::from_millis(6500))
}

thread_local! {
    /// One LS2 registration per calling thread, kept between calls.
    static CONNECTION: RefCell<Option<Connection>> = const { RefCell::new(None) };
}

pub fn call_with_timeout(uri: &str, payload: &Value, timeout: Duration) -> Result<Value, String> {
    CONNECTION.with(|slot| {
        let mut slot = slot.borrow_mut();
        let mut retried = false;
        loop {
            let connection = match slot.as_mut() {
                Some(connection) => connection,
                None => slot.insert(Connection::open()?),
            };
            match connection.request(uri, payload, timeout) {
                Ok(value) => return Ok(value),
                Err(error) if error == TIMED_OUT => {
                    // Not retried (that would double the wait), but the connection goes:
                    // unregistering is what makes a late reply to the cancelled call harmless.
                    *slot = None;
                    return Err(error);
                }
                Err(error) if !retried && error.starts_with(SUBMISSION_FAILED) => {
                    // The registration went bad (e.g. the hub restarted): open a new one, once.
                    *slot = None;
                    retried = true;
                }
                Err(error) => return Err(error),
            }
        }
    })
}

type Sink = Box<dyn FnMut(Value) + Send>;

extern "C" fn deliver(_handle: *mut c_void, message: *mut c_void, userdata: *mut c_void) -> bool {
    if message.is_null() || userdata.is_null() {
        return true;
    }
    // SAFETY: userdata is the leaked sink of a subscription whose connection lives for the
    // rest of the process, and LS2 calls this only on that subscription's worker thread.
    unsafe {
        let payload = LSMessageGetPayload(message);
        if payload.is_null() {
            return true;
        }
        let size = libc::strnlen(payload, MAX_REPLY + 1);
        if size > MAX_REPLY {
            return true;
        }
        let bytes = std::slice::from_raw_parts(payload.cast::<u8>(), size);
        if let Ok(value) = serde_json::from_slice(bytes) {
            let sink = &mut *userdata.cast::<Sink>();
            // A panic must not unwind into LS2's C frames, which would abort the process.
            if catch_unwind(AssertUnwindSafe(|| sink(value))).is_err() {
                eprintln!("Luna subscription: handler panicked");
            }
        }
    }
    true
}

/// A subscription kept for the life of the process on its own thread and LS2 connection,
/// e.g. sam's registerApp lifecycle events. Each reply goes to `on_message` on that thread.
/// Returns once the service has answered the call: an error if the connection or call fails,
/// the first reply has `returnValue: false`, or no reply comes within `timeout`.
pub fn subscribe(
    name: Option<&str>,
    uri: &str,
    payload: &Value,
    timeout: Duration,
    mut on_message: impl FnMut(Value) + Send + 'static,
) -> Result<(), String> {
    let name = name
        .map(CString::new)
        .transpose()
        .map_err(|_| "Invalid Luna service name")?;
    let uri = CString::new(uri).map_err(|_| "Invalid Luna URI")?;
    let payload = CString::new(payload.to_string()).map_err(|_| "Invalid Luna payload")?;
    let (started, handshake) = mpsc::sync_channel::<Result<(), String>>(1);
    let mut first = Some(started.clone());
    let sink: Sink = Box::new(move |value: Value| {
        if let Some(started) = first.take() {
            let result = if value.get("returnValue").and_then(Value::as_bool) == Some(false) {
                Err(format!("Luna subscription refused: {value}"))
            } else {
                Ok(())
            };
            let _ = started.send(result);
        }
        on_message(value);
    });
    thread::Builder::new()
        .name("luna-subscription".into())
        .spawn(move || {
            let connection = match Connection::open_named(name.as_deref()) {
                Ok(connection) => connection,
                Err(error) => {
                    return drop(started.send(Err(format!("Luna subscription: {error}"))));
                }
            };
            let userdata = Box::into_raw(Box::new(sink));
            let mut error = Error::new();
            let mut token = 0;
            // SAFETY: strings outlive the call; once submitted, the sink is never freed.
            if !unsafe {
                LSCall(
                    connection.handle,
                    uri.as_ptr(),
                    payload.as_ptr(),
                    deliver,
                    userdata.cast(),
                    &mut token,
                    &mut error,
                )
            } {
                // SAFETY: a refused call never registered the callback, so nothing else holds it.
                drop(unsafe { Box::from_raw(userdata) });
                let message = format!("Luna subscription: {}", error.describe("call submission"));
                return drop(started.send(Err(message)));
            }
            drop(started);
            loop {
                // SAFETY: this thread alone drives the connection's private context.
                unsafe { g_main_context_iteration(connection.context, 1) };
            }
        })
        .map_err(|error| error.to_string())?;
    match handshake.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err("Luna subscription: no reply".into()),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err("Luna subscription ended".into()),
    }
}
