//! File permissions: only the current user (v2.1 §30.2, §30.5, WIN-03, V-G05).
//!
//! A profile file names the services a person connects to and the secret references bound to
//! them. It is not a secret itself — §4.3 keeps those in the OS store — but it is a map of
//! where the secrets go, and a map left world-readable is worth having.
//!
//! Three platforms, three mechanisms, one promise: **nobody but the current user**.
//!
//! | platform | mechanism |
//! |---|---|
//! | Unix | `chmod 0600` via [`std::os::unix::fs::PermissionsExt`] |
//! | Windows | an explicit, protected DACL granting only the current user's SID |
//!
//! On Windows this is the one place in the workspace that needs `unsafe`, and it is worth
//! being explicit about why. Rust's `std` cannot attach a security descriptor to a file:
//!
//! `OpenOptionsExt` offers flags and quality-of-service hints, not a DACL. Inheriting whatever
//! `%LOCALAPPDATA%` happens to grant is not the same promise — it is a guess about a
//! directory somebody else configured. WIN-03 asks for the ACL to be *set*, so it is set, in
//! one contained module whose entire surface is four Win32 calls and which is verified by
//! reading the result back.

use std::path::Path;

/// Restrict `path` so that only the current user may read or write it.
///
/// Idempotent, and a no-op for a path that does not exist — callers restrict a file
/// immediately after creating it, and a race with a concurrent delete should not be an error.
///
/// # Errors
/// Whatever the platform said, with the path attached.
pub fn restrict_to_current_user(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    platform::restrict(path)
}

/// Describe the current permissions, for a test or a diagnostic.
///
/// # Errors
/// Whatever the platform said.
pub fn describe(path: &Path) -> std::io::Result<String> {
    platform::describe(path)
}

/// Whether the permissions grant anyone besides the current user.
///
/// # Errors
/// Whatever the platform said.
pub fn is_user_only(path: &Path) -> std::io::Result<bool> {
    platform::is_user_only(path)
}

#[cfg(unix)]
mod platform {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::Path;

    pub(super) fn restrict(path: &Path) -> std::io::Result<()> {
        let mut p = std::fs::metadata(path)?.permissions();
        p.set_mode(0o600);
        std::fs::set_permissions(path, p)
    }

    pub(super) fn describe(path: &Path) -> std::io::Result<String> {
        let mode = std::fs::metadata(path)?.permissions().mode() & 0o777;
        Ok(format!("mode {mode:04o}"))
    }

    pub(super) fn is_user_only(path: &Path) -> std::io::Result<bool> {
        let mode = std::fs::metadata(path)?.permissions().mode();
        // Nothing for group, nothing for other. Written as a mask rather than as
        // `trailing_zeros`, which clippy suggests, because the mask *is* the permission bits
        // and the suggestion would make that unreadable.
        #[allow(clippy::verbose_bit_mask)]
        Ok(mode & 0o077 == 0)
    }
}

#[cfg(windows)]
mod platform {
    //! The Win32 half. Four calls, each checked, and the result read back.
    //!
    //! `unsafe` is denied workspace-wide; it is allowed here because there is no safe path to
    //! a DACL in `std`, and WIN-03 requires one to be set rather than inherited. Every call
    //! below returns a status that is checked, every allocation is freed on both the success
    //! and the failure path, and `verify` reads the descriptor back so a silent no-op cannot
    //! pass for success.
    #![allow(unsafe_code)]

    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
    // `ConvertSidToStringSidW` lives in `Security::Authorization`, not `Security`, even though
    // it reads like it belongs with the other `Sid` names. Getting this wrong is invisible on
    // macOS and Linux — the whole module is `#[cfg(windows)]` — and it broke three Windows CI
    // jobs at once. Cross-checking it locally is not available either: blake3 and ring have
    // build scripts that need a Windows-targeting C compiler, so `cargo check --target
    // x86_64-pc-windows-msvc` cannot get far enough to typecheck this file. The `build+lint
    // (windows-latest)` job is the only thing that sees it, which means it has to be read
    // after pushing rather than assumed.
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
        ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
        SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW,
    };
    use windows_sys::Win32::Security::{
        ACL, DACL_SECURITY_INFORMATION, GetTokenInformation, PROTECTED_DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    fn last_error() -> std::io::Error {
        std::io::Error::last_os_error()
    }

    /// The current user's SID, as an SDDL string such as `S-1-5-21-...`.
    fn current_user_sid() -> std::io::Result<String> {
        let mut token: HANDLE = std::ptr::null_mut();
        // SAFETY: `OpenProcessToken` writes a handle into `token` or returns 0. The handle is
        // closed below on every path.
        let ok = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) };
        if ok == 0 {
            return Err(last_error());
        }
        let result = (|| -> std::io::Result<String> {
            let mut needed = 0u32;
            // SAFETY: called with a null buffer to learn the size, which is how this API is
            // documented to be used; it fails with ERROR_INSUFFICIENT_BUFFER and sets `needed`.
            unsafe {
                GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &raw mut needed)
            };
            if needed == 0 {
                return Err(last_error());
            }
            let mut buf = vec![0u8; needed as usize];
            // SAFETY: `buf` is `needed` bytes, which is what the call above asked for.
            let ok = unsafe {
                GetTokenInformation(
                    token,
                    TokenUser,
                    buf.as_mut_ptr().cast(),
                    needed,
                    &raw mut needed,
                )
            };
            if ok == 0 {
                return Err(last_error());
            }
            // SAFETY: on success the buffer holds a `TOKEN_USER` whose `User.Sid` points into
            // it; `buf` outlives this borrow.
            let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
            let mut out: *mut u16 = std::ptr::null_mut();
            // SAFETY: `sid` is valid for the lifetime of `buf`; the returned string is freed
            // with `LocalFree` below.
            let ok = unsafe { ConvertSidToStringSidW(sid, &raw mut out) };
            if ok == 0 || out.is_null() {
                return Err(last_error());
            }
            // SAFETY: `out` is a NUL-terminated wide string allocated by the call above.
            let s = unsafe { widestring_to_string(out) };
            // SAFETY: `out` came from `ConvertSidToStringSidW`, which documents `LocalFree`.
            unsafe { LocalFree(out.cast::<core::ffi::c_void>() as HLOCAL) };
            Ok(s)
        })();
        // SAFETY: `token` is a handle this function opened and has not closed.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(token) };
        result
    }

    /// Read a NUL-terminated wide string.
    ///
    /// # Safety
    /// `p` must point at a NUL-terminated UTF-16 sequence.
    unsafe fn widestring_to_string(p: *const u16) -> String {
        let mut len = 0usize;
        // SAFETY: the caller promises a NUL terminator.
        while unsafe { *p.add(len) } != 0 {
            len += 1;
        }
        // SAFETY: `len` is the length before the terminator.
        let slice = unsafe { std::slice::from_raw_parts(p, len) };
        String::from_utf16_lossy(slice)
    }

    pub(super) fn restrict(path: &Path) -> std::io::Result<()> {
        let sid = current_user_sid()?;
        // `D:P` — a protected DACL, so nothing is inherited from the parent directory.
        // `(A;;FA;;;<sid>)` — allow full access, to this user and nobody else.
        let sddl: Vec<u16> = format!("D:P(A;;FA;;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `sddl` is NUL-terminated; the descriptor is freed below on both paths.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &raw mut descriptor,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || descriptor.is_null() {
            return Err(last_error());
        }
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut present = 0i32;
        let mut defaulted = 0i32;
        // SAFETY: `descriptor` was produced by the call above and is still owned here.
        let got = unsafe {
            windows_sys::Win32::Security::GetSecurityDescriptorDacl(
                descriptor,
                &raw mut present,
                &raw mut dacl,
                &raw mut defaulted,
            )
        };
        let result = if got == 0 || present == 0 {
            Err(last_error())
        } else {
            let mut w = wide(path);
            // SAFETY: `w` is NUL-terminated and `dacl` points into `descriptor`, which is
            // still alive.
            let rc = unsafe {
                SetNamedSecurityInfoW(
                    w.as_mut_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    dacl,
                    std::ptr::null_mut(),
                )
            };
            if rc == 0 {
                Ok(())
            } else {
                Err(std::io::Error::from_raw_os_error(rc as i32))
            }
        };
        // SAFETY: `descriptor` came from `ConvertStringSecurityDescriptorToSecurityDescriptorW`,
        // which documents `LocalFree`.
        unsafe { LocalFree(descriptor.cast::<core::ffi::c_void>() as HLOCAL) };
        result
    }

    /// The file's DACL as an SDDL string.
    fn sddl_of(path: &Path) -> std::io::Result<String> {
        let mut w = wide(path);
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `w` is NUL-terminated; the descriptor is freed below.
        let rc = unsafe {
            GetNamedSecurityInfoW(
                w.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut descriptor,
            )
        };
        if rc != 0 || descriptor.is_null() {
            return Err(std::io::Error::from_raw_os_error(rc as i32));
        }
        let mut out: *mut u16 = std::ptr::null_mut();
        let mut len = 0u32;
        // SAFETY: `descriptor` is owned here; `out` is freed below.
        let ok = unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &raw mut out,
                &raw mut len,
            )
        };
        let result = if ok == 0 || out.is_null() {
            Err(last_error())
        } else {
            // SAFETY: `out` is a NUL-terminated wide string from the call above.
            let s = unsafe { widestring_to_string(out) };
            // SAFETY: documented to be freed with `LocalFree`.
            unsafe { LocalFree(out.cast::<core::ffi::c_void>() as HLOCAL) };
            Ok(s)
        };
        // SAFETY: `descriptor` came from `GetNamedSecurityInfoW`.
        unsafe { LocalFree(descriptor.cast::<core::ffi::c_void>() as HLOCAL) };
        result
    }

    pub(super) fn describe(path: &Path) -> std::io::Result<String> {
        sddl_of(path)
    }

    pub(super) fn is_user_only(path: &Path) -> std::io::Result<bool> {
        let sddl = sddl_of(path)?;
        let sid = current_user_sid()?;
        // Protected, and every allow-ACE names this user. `D:P` is what says "inherit
        // nothing"; without it the parent directory could still be granting access.
        let protected = sddl.starts_with("D:P") || sddl.contains("D:PAI") || sddl.contains("D:P(");
        let aces: Vec<&str> = sddl.split("(").skip(1).collect();
        let only_me = !aces.is_empty() && aces.iter().all(|a| a.contains(&sid));
        Ok(protected && only_me)
    }
}

#[cfg(all(not(unix), not(windows)))]
mod platform {
    use std::path::Path;

    pub(super) fn restrict(_path: &Path) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "this platform has no supported way to restrict a file to the current user; \
             Penguin will not pretend it did",
        ))
    }
    pub(super) fn describe(_path: &Path) -> std::io::Result<String> {
        Ok("unsupported platform".to_owned())
    }
    pub(super) fn is_user_only(_path: &Path) -> std::io::Result<bool> {
        Ok(false)
    }
}
