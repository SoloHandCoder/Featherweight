//! The real `System`: registry, services, scheduled tasks, Appx packages,
//! power schemes and PowerShell on this machine.

use std::os::windows::process::CommandExt;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use std::sync::OnceLock;

use ::windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_SERVICE_CANNOT_ACCEPT_CTRL,
    ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_NOT_ACTIVE, HANDLE,
};
use ::windows::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use ::windows::Win32::System::Services::{
    ChangeServiceConfigW, CloseServiceHandle, ControlService, ENUM_SERVICE_TYPE, OpenSCManagerW,
    OpenServiceW, QUERY_SERVICE_CONFIGW, QueryServiceConfigW, SC_HANDLE, SC_MANAGER_CONNECT,
    SERVICE_AUTO_START, SERVICE_CHANGE_CONFIG, SERVICE_CONTROL_STOP, SERVICE_DEMAND_START,
    SERVICE_DISABLED, SERVICE_ERROR, SERVICE_NO_CHANGE, SERVICE_QUERY_CONFIG, SERVICE_START_TYPE,
    SERVICE_STATUS, SERVICE_STOP,
};
use ::windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use ::windows::core::{HSTRING, PCWSTR};
use regex::Regex;
use winreg::RegKey;
use winreg::RegValue;
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY, KEY_WRITE,
    REG_BINARY, REG_DWORD, REG_QWORD, REG_SZ,
};

use super::{SysError, System};
use crate::model::{Hive, RegData, StartType};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Talks to the live Windows installation.
pub struct WindowsSystem;

// ---------------------------------------------------------------- helpers

fn win32_code(e: &::windows::core::Error) -> u32 {
    let c = e.code().0 as u32;
    if c >> 16 == 0x8007 { c & 0xffff } else { c }
}

fn map_win(e: ::windows::core::Error, what: &str) -> SysError {
    if win32_code(&e) == ERROR_ACCESS_DENIED.0 {
        SysError::AccessDenied(format!("{what}: {e}"))
    } else {
        SysError::Other(format!("{what}: {e}"))
    }
}

fn map_io(e: std::io::Error, what: &str) -> SysError {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        SysError::AccessDenied(format!("{what}: {e}"))
    } else {
        SysError::Other(format!("{what}: {e}"))
    }
}

/// Whether this process runs with an elevated token.
fn is_elevated() -> bool {
    let mut token = HANDLE::default();
    // SAFETY: pseudo-handle and a valid out-pointer; the token is closed below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let mut elev = TOKEN_ELEVATION::default();
    let mut ret = 0u32;
    // SAFETY: `elev` is a correctly sized TOKEN_ELEVATION buffer.
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elev as *mut TOKEN_ELEVATION).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret,
        )
    }
    .is_ok();
    // SAFETY: `token` was opened above and is closed exactly once.
    unsafe {
        let _ = CloseHandle(token);
    }
    ok && elev.TokenIsElevated != 0
}

struct Output {
    ok: bool,
    stdout: String,
    stderr: String,
}

fn system32() -> std::path::PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    std::path::Path::new(&root).join("System32")
}

const TOOL_TIMEOUT: Duration = Duration::from_secs(120);

fn drain<R: Read + Send + 'static>(mut r: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = r.read_to_end(&mut v);
        v
    })
}

/// Runs a console tool without a window; arguments are passed separately.
/// Killed after `TOOL_TIMEOUT`.
fn run_tool(exe: &std::path::Path, args: &[&str]) -> Result<Output, SysError> {
    let name = exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut child = Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| map_io(e, &exe.display().to_string()))?;
    let out = drain(child.stdout.take().expect("piped"));
    let err = drain(child.stderr.take().expect("piped"));
    let deadline = Instant::now() + TOOL_TIMEOUT;
    let status = loop {
        match child.try_wait().map_err(|e| map_io(e, &name))? {
            Some(st) => break st,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SysError::Other(format!(
                    "timed out after {}s running {name}",
                    TOOL_TIMEOUT.as_secs()
                )));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    Ok(Output {
        ok: status.success(),
        stdout: decode(&out.join().unwrap_or_default()),
        stderr: decode(&err.join().unwrap_or_default()),
    })
}

/// Console output may be UTF-16 (with or without BOM) or an 8-bit code page.
fn decode(bytes: &[u8]) -> String {
    let utf16 = |b: &[u8]| {
        let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    };
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        return utf16(&bytes[2..]);
    }
    // BOM-less UTF-16LE of mostly-ASCII text is full of NUL bytes.
    let head = &bytes[..bytes.len().min(64)];
    if head.len() >= 4 && head.iter().filter(|&&b| b == 0).count() * 4 >= head.len() {
        return utf16(bytes);
    }
    String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes)).into_owned()
}

/// Message text of a PowerShell CLIXML error stream, or the text unchanged.
fn strip_clixml(s: &str) -> String {
    if !s.trim_start().starts_with("#< CLIXML") {
        return s.to_string();
    }
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"<S S="Error">(.*?)</S>"#).unwrap());
    re.captures_iter(s)
        .map(|c| c[1].replace("_x000D_", "").replace("_x000A_", " "))
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string()
}

fn text_of(o: &Output) -> String {
    let err = strip_clixml(&o.stderr);
    let s = if err.trim().is_empty() { &o.stdout } else { &err };
    s.trim().to_string()
}

fn looks_denied(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("access is denied") || t.contains("unauthorizedaccess") || t.contains("0x80070005")
}

/// Error for a failed tool. When `denied_if_unelevated`, any failure without
/// admin rights is reported as access denied (the text is localized, so it
/// can't be relied on).
fn tool_err(what: &str, o: &Output, denied_if_unelevated: bool) -> SysError {
    let text = text_of(o);
    let msg = format!("{what} failed: {text}");
    if looks_denied(&text) || (denied_if_unelevated && !is_elevated()) {
        SysError::AccessDenied(msg)
    } else {
        SysError::Other(msg)
    }
}

fn run_ps(script: &str) -> Result<Output, SysError> {
    let exe = system32().join(r"WindowsPowerShell\v1.0\powershell.exe");
    let full = format!(
        "$ProgressPreference='SilentlyContinue';[Console]::OutputEncoding=[Text.Encoding]::UTF8;{script}"
    );
    let utf16le: Vec<u8> = full.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64(&utf16le);
    run_tool(
        &exe,
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &encoded],
    )
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

fn package_name(p: &str) -> Result<&str, SysError> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[A-Za-z0-9._-]+$").unwrap());
    if re.is_match(p) {
        Ok(p)
    } else {
        Err(SysError::Other(format!("invalid package name: {p:?}")))
    }
}

fn guid_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap()
    })
}

fn checked_guid(g: &str) -> Result<String, SysError> {
    match guid_re().find(g) {
        Some(m) if m.start() == 0 && m.end() == g.len() => Ok(g.to_lowercase()),
        _ => Err(SysError::Other(format!("invalid power scheme GUID: {g:?}"))),
    }
}

fn powercfg() -> std::path::PathBuf {
    system32().join("powercfg.exe")
}

fn schtasks() -> std::path::PathBuf {
    system32().join("schtasks.exe")
}

// --------------------------------------------------------------- registry

fn root(hive: Hive) -> RegKey {
    RegKey::predef(match hive {
        Hive::Hklm => HKEY_LOCAL_MACHINE,
        Hive::Hkcu => HKEY_CURRENT_USER,
    })
}

fn reg_to_data(v: RegValue) -> Result<RegData, SysError> {
    use winreg::types::FromRegValue;
    let bad = |e: std::io::Error| SysError::Other(format!("malformed registry value: {e}"));
    Ok(match v.vtype {
        REG_DWORD => RegData::Dword(u32::from_reg_value(&v).map_err(bad)?),
        REG_QWORD => RegData::Qword(u64::from_reg_value(&v).map_err(bad)?),
        REG_SZ => RegData::String(String::from_reg_value(&v).map_err(bad)?),
        REG_BINARY => RegData::Binary(v.bytes.to_vec()),
        other => {
            return Err(SysError::Other(format!("unsupported registry type {other:?}")));
        }
    })
}

fn data_to_reg(d: &RegData) -> RegValue<'_> {
    use winreg::types::ToRegValue;
    match d {
        RegData::Dword(n) => n.to_reg_value(),
        RegData::Qword(n) => n.to_reg_value(),
        RegData::String(s) => s.to_reg_value(),
        RegData::Binary(b) => RegValue { bytes: b.clone().into(), vtype: REG_BINARY },
    }
}

fn is_missing(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound
}

// --------------------------------------------------------------- services

struct Sc(SC_HANDLE);
impl Drop for Sc {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by us and is closed exactly once.
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

fn open_scm() -> Result<Sc, SysError> {
    // SAFETY: null machine/database names select the local active database.
    unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }
        .map(Sc)
        .map_err(|e| map_win(e, "OpenSCManager"))
}

/// `Ok(None)` when the service does not exist.
fn open_service(scm: &Sc, name: &str, access: u32) -> Result<Option<Sc>, SysError> {
    let n = HSTRING::from(name);
    // SAFETY: `n` outlives the call.
    match unsafe { OpenServiceW(scm.0, &n, access) } {
        Ok(h) => Ok(Some(Sc(h))),
        Err(e) if win32_code(&e) == ERROR_SERVICE_DOES_NOT_EXIST.0 => Ok(None),
        Err(e) => Err(map_win(e, &format!("OpenService {name}"))),
    }
}

// ------------------------------------------------------------ scheduled tasks

fn check_task_path(path: &str) -> Result<(), SysError> {
    if path.is_empty()
        || path.starts_with('/')
        || path.starts_with('-')
        || path.contains(['"', '\0', '\n', '\r'])
    {
        return Err(SysError::Other(format!("invalid task path: {path:?}")));
    }
    Ok(())
}

/// Whether the Task Scheduler cache knows `path` (readable without elevation,
/// unlike the task XML of system tasks).
fn task_exists(path: &str) -> Result<bool, SysError> {
    let sub = format!(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\TaskCache\Tree\{}",
        path.trim_start_matches('\\')
    );
    match root(Hive::Hklm).open_subkey_with_flags(&sub, KEY_READ | KEY_WOW64_64KEY) {
        Ok(_) => Ok(true),
        Err(e) if is_missing(&e) => Ok(false),
        Err(e) => Err(map_io(e, "task cache")),
    }
}

/// Task-level `<Settings><Enabled>`; absent inside `<Settings>` means
/// enabled. Fails closed: output that isn't a task definition with a
/// `<Settings>` element is an error. (Trigger elements also carry
/// `<Enabled>`, so only the `<Settings>` block is searched.)
fn enabled_from_xml(xml: &str) -> Result<bool, SysError> {
    let bad = |what: &str| SysError::Other(format!("unexpected task definition: {what}"));
    if !xml.contains("<Task") {
        return Err(bad("no <Task> element"));
    }
    let start = xml.find("<Settings>").ok_or_else(|| bad("no <Settings> element"))?;
    let rest = &xml[start..];
    let block = &rest[..rest.find("</Settings>").ok_or_else(|| bad("unterminated <Settings>"))?];
    Ok(match block.find("<Enabled>") {
        Some(i) => !block[i + "<Enabled>".len()..].trim_start().to_lowercase().starts_with("false"),
        None => true,
    })
}

// ------------------------------------------------------------------ System

impl System for WindowsSystem {
    fn reg_get(&self, hive: Hive, path: &str, name: &str) -> Result<Option<RegData>, SysError> {
        let key = match root(hive).open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY) {
            Ok(k) => k,
            Err(e) if is_missing(&e) => return Ok(None),
            Err(e) => return Err(map_io(e, &format!("open {path}"))),
        };
        match key.get_raw_value(name) {
            Ok(v) => reg_to_data(v).map(Some),
            Err(e) if is_missing(&e) => Ok(None),
            Err(e) => Err(map_io(e, &format!("read {path}\\{name}"))),
        }
    }

    fn reg_set(
        &mut self,
        hive: Hive,
        path: &str,
        name: &str,
        value: Option<&RegData>,
    ) -> Result<(), SysError> {
        match value {
            Some(d) => {
                let (key, _) = root(hive)
                    .create_subkey_with_flags(path, KEY_WRITE | KEY_WOW64_64KEY)
                    .map_err(|e| map_io(e, &format!("create {path}")))?;
                key.set_raw_value(name, &data_to_reg(d))
                    .map_err(|e| map_io(e, &format!("write {path}\\{name}")))
            }
            None => {
                let key = match root(hive).open_subkey_with_flags(path, KEY_SET_VALUE | KEY_WOW64_64KEY)
                {
                    Ok(k) => k,
                    Err(e) if is_missing(&e) => return Ok(()),
                    Err(e) => return Err(map_io(e, &format!("open {path}"))),
                };
                match key.delete_value(name) {
                    Ok(()) => Ok(()),
                    Err(e) if is_missing(&e) => Ok(()),
                    Err(e) => Err(map_io(e, &format!("delete {path}\\{name}"))),
                }
            }
        }
    }

    fn service_start(&self, name: &str) -> Result<Option<StartType>, SysError> {
        let scm = open_scm()?;
        let Some(svc) = open_service(&scm, name, SERVICE_QUERY_CONFIG)? else {
            return Ok(None);
        };
        let mut needed = 0u32;
        // SAFETY: size probe; a null buffer is allowed with size 0.
        let _ = unsafe { QueryServiceConfigW(svc.0, None, 0, &mut needed) };
        if needed == 0 {
            return Err(SysError::Other(format!("QueryServiceConfig {name}: no size")));
        }
        // 8-byte aligned storage for QUERY_SERVICE_CONFIGW.
        let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
        let cfg = buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
        // SAFETY: `buf` holds at least `needed` bytes and is suitably aligned.
        unsafe { QueryServiceConfigW(svc.0, Some(cfg), needed, &mut needed) }
            .map_err(|e| map_win(e, &format!("QueryServiceConfig {name}")))?;
        // SAFETY: the call above filled the struct.
        let start = unsafe { (*cfg).dwStartType };
        // Automatic and Automatic (Delayed Start) share dwStartType == AUTO_START.
        match start {
            SERVICE_AUTO_START => Ok(Some(StartType::Automatic)),
            SERVICE_DEMAND_START => Ok(Some(StartType::Manual)),
            SERVICE_DISABLED => Ok(Some(StartType::Disabled)),
            other => Err(SysError::Other(format!(
                "service {name} has unsupported start type {}",
                other.0
            ))),
        }
    }

    fn set_service_start(&mut self, name: &str, start: StartType) -> Result<(), SysError> {
        let scm = open_scm()?;
        let access = if start == StartType::Disabled {
            SERVICE_CHANGE_CONFIG | SERVICE_STOP
        } else {
            SERVICE_CHANGE_CONFIG
        };
        let Some(svc) = open_service(&scm, name, access)? else {
            return Err(SysError::NotFound(format!("service {name}")));
        };
        let st: SERVICE_START_TYPE = match start {
            StartType::Automatic => SERVICE_AUTO_START,
            StartType::Manual => SERVICE_DEMAND_START,
            StartType::Disabled => SERVICE_DISABLED,
        };
        // SAFETY: SERVICE_NO_CHANGE / null leave every other setting as it is.
        unsafe {
            ChangeServiceConfigW(
                svc.0,
                ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
                st,
                SERVICE_ERROR(SERVICE_NO_CHANGE),
                PCWSTR::null(),
                PCWSTR::null(),
                None,
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR::null(),
            )
        }
        .map_err(|e| map_win(e, &format!("ChangeServiceConfig {name}")))?;
        if start == StartType::Disabled {
            let mut status = SERVICE_STATUS::default();
            // SAFETY: valid handle and out-pointer.
            if let Err(e) = unsafe { ControlService(svc.0, SERVICE_CONTROL_STOP, &mut status) } {
                // Not running / not stoppable: the config change already took.
                let c = win32_code(&e);
                if c != ERROR_SERVICE_NOT_ACTIVE.0 && c != ERROR_SERVICE_CANNOT_ACCEPT_CTRL.0 {
                    return Err(map_win(e, &format!("stop {name}")));
                }
            }
        }
        Ok(())
    }

    fn task_enabled(&self, path: &str) -> Result<Option<bool>, SysError> {
        check_task_path(path)?;
        if !task_exists(path)? {
            return Ok(None);
        }
        let o = run_tool(&schtasks(), &["/Query", "/TN", path, "/XML"])?;
        if !o.ok {
            return Err(tool_err("schtasks /Query", &o, true));
        }
        enabled_from_xml(&o.stdout).map(Some)
    }

    fn set_task_enabled(&mut self, path: &str, enabled: bool) -> Result<(), SysError> {
        check_task_path(path)?;
        let flag = if enabled { "/ENABLE" } else { "/DISABLE" };
        let o = run_tool(&schtasks(), &["/Change", "/TN", path, flag])?;
        if o.ok {
            return Ok(());
        }
        if !task_exists(path)? {
            return Err(SysError::NotFound(format!("task {path}")));
        }
        Err(tool_err("schtasks /Change", &o, true))
    }

    /// `Some(true)` iff installed for the current user, else `Some(false)`.
    fn appx_installed(&self, package: &str) -> Result<Option<bool>, SysError> {
        let p = package_name(package)?;
        let script = format!(
            "$ErrorActionPreference='Stop';\
             if (@(Get-AppxPackage -Name '{p}').Count -gt 0) {{ 'yes' }} else {{ 'no' }}"
        );
        let o = run_ps(&script)?;
        if !o.ok {
            return Err(tool_err("Get-AppxPackage", &o, false));
        }
        match o.stdout.trim() {
            "yes" => Ok(Some(true)),
            "no" => Ok(Some(false)),
            other => Err(SysError::Other(format!("unexpected appx query output: {other:?}"))),
        }
    }

    fn remove_appx(&mut self, package: &str, all_users: bool) -> Result<(), SysError> {
        let p = package_name(package)?;
        let script = if all_users {
            format!(
                "$ErrorActionPreference='Stop';\
                 Get-AppxPackage -AllUsers -Name '{p}' | Remove-AppxPackage -AllUsers;\
                 Get-AppxProvisionedPackage -Online | Where-Object {{ $_.DisplayName -eq '{p}' }} \
                 | Remove-AppxProvisionedPackage -Online | Out-Null"
            )
        } else {
            format!(
                "$ErrorActionPreference='Stop';\
                 Get-AppxPackage -Name '{p}' | Remove-AppxPackage"
            )
        };
        let o = run_ps(&script)?;
        if o.ok { Ok(()) } else { Err(tool_err("Remove-AppxPackage", &o, all_users)) }
    }

    fn active_power_scheme(&self) -> Result<String, SysError> {
        let o = run_tool(&powercfg(), &["/getactivescheme"])?;
        if !o.ok {
            return Err(tool_err("powercfg /getactivescheme", &o, false));
        }
        guid_re()
            .find(&o.stdout)
            .map(|m| m.as_str().to_lowercase())
            .ok_or_else(|| SysError::Other(format!("no GUID in powercfg output: {}", o.stdout.trim())))
    }

    fn ensure_power_scheme(
        &mut self,
        guid: &str,
        duplicate_from: Option<&str>,
    ) -> Result<(), SysError> {
        let guid = checked_guid(guid)?;
        let list = run_tool(&powercfg(), &["/list"])?;
        if !list.ok {
            return Err(tool_err("powercfg /list", &list, false));
        }
        if list.stdout.to_lowercase().contains(&guid) {
            return Ok(());
        }
        let Some(from) = duplicate_from else {
            return Err(SysError::NotFound(format!("power scheme {guid}")));
        };
        let from = checked_guid(from)?;
        let o = run_tool(&powercfg(), &["/duplicatescheme", &from, &guid])?;
        if o.ok { Ok(()) } else { Err(tool_err("powercfg /duplicatescheme", &o, false)) }
    }

    fn set_power_scheme(&mut self, guid: &str) -> Result<(), SysError> {
        let guid = checked_guid(guid)?;
        let o = run_tool(&powercfg(), &["/setactive", &guid])?;
        if o.ok { Ok(()) } else { Err(tool_err("powercfg /setactive", &o, false)) }
    }

    fn run_powershell(&mut self, script: &str) -> Result<String, SysError> {
        let o = run_ps(script)?;
        if o.ok { Ok(o.stdout.trim().to_string()) } else { Err(tool_err("powershell", &o, false)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_enabled_reads_settings_block_only() {
        let on = "<Task><Triggers><TimeTrigger><Enabled>false</Enabled></TimeTrigger></Triggers><Settings><Enabled>true</Enabled></Settings></Task>";
        assert!(enabled_from_xml(on).unwrap());
        let off = "<Task><Triggers><X><Enabled>true</Enabled></X></Triggers><Settings><AllowStartOnDemand>true</AllowStartOnDemand><Enabled>false</Enabled></Settings></Task>";
        assert!(!enabled_from_xml(off).unwrap());
        assert!(enabled_from_xml("<Task><Settings><Hidden>false</Hidden></Settings></Task>").unwrap());
    }

    #[test]
    fn xml_enabled_fails_closed() {
        assert!(matches!(enabled_from_xml(""), Err(SysError::Other(_))));
        assert!(matches!(enabled_from_xml("ERROR: garbage"), Err(SysError::Other(_))));
        assert!(matches!(enabled_from_xml("<Task></Task>"), Err(SysError::Other(_))));
        assert!(matches!(
            enabled_from_xml("<Task><Settings><Enabled>false</Enabled>"),
            Err(SysError::Other(_))
        ));
    }

    #[test]
    fn decode_handles_utf16_with_and_without_bom() {
        let u: Vec<u8> = "<Task>x</Task>".encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(decode(&u), "<Task>x</Task>");
        let mut bom = vec![0xFF, 0xFE];
        bom.extend(&u);
        assert_eq!(decode(&bom), "<Task>x</Task>");
        assert_eq!(decode(b"plain text"), "plain text");
    }

    #[test]
    fn clixml_is_reduced_to_message() {
        let s = "#< CLIXML\r\n<Objs><S S=\"Error\">boom_x000D__x000A_</S><S S=\"Error\">more</S></Objs>";
        assert_eq!(strip_clixml(s), "boom more");
        assert_eq!(strip_clixml("plain"), "plain");
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
    }

    #[test]
    fn package_names_are_validated() {
        assert!(package_name("Microsoft.BingNews").is_ok());
        assert!(package_name("a'; calc #").is_err());
        assert!(package_name("").is_err());
    }

    #[test]
    fn guid_is_found_in_localized_output() {
        let s = "Schema de gestion: 381B4222-F694-41F0-9685-FF5BB260DF2E  (Performances)";
        assert_eq!(
            guid_re().find(s).unwrap().as_str().to_lowercase(),
            "381b4222-f694-41f0-9685-ff5bb260df2e"
        );
        assert!(checked_guid("381b4222-f694-41f0-9685-ff5bb260df2e").is_ok());
        assert!(checked_guid("/x 381b4222-f694-41f0-9685-ff5bb260df2e").is_err());
    }
}
