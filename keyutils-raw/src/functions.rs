// Copyright (c) 2018, Ben Boeckel
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without modification,
// are permitted provided that the following conditions are met:
//
//     * Redistributions of source code must retain the above copyright notice,
//       this list of conditions and the following disclaimer.
//     * Redistributions in binary form must reproduce the above copyright notice,
//       this list of conditions and the following disclaimer in the documentation
//       and/or other materials provided with the distribution.
//     * Neither the name of this project nor the names of its contributors
//       may be used to endorse or promote products derived from this software
//       without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
// WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR
// ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES
// (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
// LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON
// ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS
// SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::convert::TryInto;
use std::ffi::CString;
use std::ptr;

use log::error;
use uninit::out_ref::Out;

use crate::{DefaultKeyring, KeyPermissions, KeyringSerial, TimeoutSeconds};

/// Reexport of `Errno` as `Error`.
type Error = errno::Errno;
/// Simpler `Result` type with the error already set.
type Result<T> = std::result::Result<T, Error>;

fn check_syscall(res: libc::c_long) -> Result<libc::c_long> {
    if res == -1 {
        Err(errno::errno())
    } else {
        Ok(res)
    }
}

static THE_KERNEL_LIED: &str = concat!(
    "It appears as though the kernel made a 64-bit key ID. Please report a bug.\n\n",
    env!("CARGO_PKG_REPOSITORY"),
);
static ZERO_KEY_ID_FOUND: &str = concat!(
    "It appears as though a key ID of zero was found. This is novel and should not happen. Please \
     report a bug.\n\n",
    env!("CARGO_PKG_REPOSITORY"),
);
static BUFFER_OVERFLOW: &str = concat!(
    "The kernel returned a size that could not be represented as a `usize`. This should not be \
     possible. Please report a bug.\n\n",
    env!("CARGO_PKG_REPOSITORY"),
);

fn cstring(s: &str) -> CString {
    CString::new(s.as_bytes()).unwrap()
}

fn opt_cstring(opt: Option<&str>) -> Option<CString> {
    opt.map(cstring)
}

fn opt_cstring_ptr(opt: &Option<CString>) -> *const libc::c_char {
    opt.as_ref().map_or(ptr::null(), |cs| cs.as_ptr())
}

fn opt_key_serial(opt: Option<KeyringSerial>) -> i32 {
    opt.map(KeyringSerial::get).unwrap_or(0)
}

fn keyring_serial(res: libc::c_long) -> KeyringSerial {
    KeyringSerial::new(res.try_into().expect(THE_KERNEL_LIED)).expect(ZERO_KEY_ID_FOUND)
}

fn default_keyring(res: libc::c_long) -> Result<DefaultKeyring> {
    res.try_into().map_err(|err: crate::UnknownDefault| {
        error!(
            concat!(
                "The kernel has returned an unexpected default keyring ID: {}. Please report a \
                 bug.\n\n",
                env!("CARGO_PKG_REPOSITORY"),
            ),
            err.0,
        );
        errno::Errno(libc::EINVAL)
    })
}

fn size(res: libc::c_long) -> usize {
    res.try_into().expect(BUFFER_OVERFLOW)
}

fn ignore(res: libc::c_long) {
    assert_eq!(res, 0);
}

fn safe_len<T>(len: usize) -> Result<T>
where
    usize: TryInto<T>,
{
    len.try_into().map_err(|_| errno::Errno(libc::EINVAL))
}

macro_rules! syscall {
    ( $( $arg:expr, )* ) => {
        check_syscall(libc::syscall($( $arg, )*))
    };
}

macro_rules! keyctl {
    ( $( $arg:expr, )* ) => {
        syscall!(libc::SYS_keyctl, $( $arg, )*)
    };
}

pub fn add_key(
    type_: &str,
    description: &str,
    payload: &[u8],
    keyring: KeyringSerial,
) -> Result<KeyringSerial> {
    let type_cstr = cstring(type_);
    let desc_cstr = cstring(description);
    unsafe {
        syscall!(
            libc::SYS_add_key,
            type_cstr.as_ptr(),
            desc_cstr.as_ptr(),
            payload.as_ptr() as *const libc::c_void,
            payload.len(),
            keyring.get(),
        )
    }
    .map(keyring_serial)
}

pub fn request_key(
    type_: &str,
    description: &str,
    callout_info: Option<&str>,
    keyring: Option<KeyringSerial>,
) -> Result<KeyringSerial> {
    let type_cstr = cstring(type_);
    let desc_cstr = cstring(description);
    let callout_cstr = opt_cstring(callout_info);
    let callout_ptr = opt_cstring_ptr(&callout_cstr);

    unsafe {
        syscall!(
            libc::SYS_request_key,
            type_cstr.as_ptr(),
            desc_cstr.as_ptr(),
            callout_ptr,
            opt_key_serial(keyring),
        )
    }
    .map(keyring_serial)
}

pub fn keyctl_get_keyring_id(id: KeyringSerial, create: bool) -> Result<KeyringSerial> {
    unsafe {
        keyctl!(
            libc::KEYCTL_GET_KEYRING_ID,
            id.get(),
            if create { 1 } else { 0 },
        )
    }
    .map(keyring_serial)
}

pub fn keyctl_join_session_keyring(name: Option<&str>) -> Result<KeyringSerial> {
    let name_cstr = opt_cstring(name);
    let name_ptr = opt_cstring_ptr(&name_cstr);

    unsafe { keyctl!(libc::KEYCTL_JOIN_SESSION_KEYRING, name_ptr,) }.map(keyring_serial)
}

pub fn keyctl_update(id: KeyringSerial, payload: &[u8]) -> Result<()> {
    unsafe {
        keyctl!(
            libc::KEYCTL_UPDATE,
            id.get(),
            payload.as_ptr() as *const libc::c_void,
            payload.len(),
        )
    }
    .map(ignore)
}

pub fn keyctl_revoke(id: KeyringSerial) -> Result<()> {
    unsafe { keyctl!(libc::KEYCTL_REVOKE, id.get(),) }.map(ignore)
}

pub fn keyctl_chown(
    id: KeyringSerial,
    uid: Option<libc::uid_t>,
    gid: Option<libc::gid_t>,
) -> Result<()> {
    unsafe {
        keyctl!(
            libc::KEYCTL_CHOWN,
            id.get(),
            uid.unwrap_or(!0),
            gid.unwrap_or(!0),
        )
    }
    .map(ignore)
}

pub fn keyctl_setperm(id: KeyringSerial, perm: KeyPermissions) -> Result<()> {
    unsafe { keyctl!(libc::KEYCTL_SETPERM, id.get(), perm,) }.map(ignore)
}

pub fn keyctl_describe(id: KeyringSerial, mut buffer: Option<Out<[u8]>>) -> Result<usize> {
    let capacity = buffer.as_mut().map_or(0, |b| b.len());
    unsafe {
        keyctl!(
            libc::KEYCTL_DESCRIBE,
            id.get(),
            buffer.as_mut().map_or(ptr::null(), |b| b.as_mut_ptr()),
            capacity,
        )
    }
    .map(size)
}

pub fn keyctl_clear(id: KeyringSerial) -> Result<()> {
    unsafe { keyctl!(libc::KEYCTL_CLEAR, id.get(),) }.map(ignore)
}

pub fn keyctl_link(id: KeyringSerial, ringid: KeyringSerial) -> Result<()> {
    unsafe { keyctl!(libc::KEYCTL_LINK, id.get(), ringid.get(),) }.map(ignore)
}

pub fn keyctl_unlink(id: KeyringSerial, ringid: KeyringSerial) -> Result<()> {
    unsafe { keyctl!(libc::KEYCTL_UNLINK, id.get(), ringid.get(),) }.map(ignore)
}

pub fn keyctl_search(
    ringid: KeyringSerial,
    type_: &str,
    description: &str,
    destringid: Option<KeyringSerial>,
) -> Result<KeyringSerial> {
    let type_cstr = cstring(type_);
    let desc_cstr = cstring(description);
    unsafe {
        keyctl!(
            libc::KEYCTL_SEARCH,
            ringid.get(),
            type_cstr.as_ptr(),
            desc_cstr.as_ptr(),
            opt_key_serial(destringid),
        )
    }
    .map(keyring_serial)
}

pub fn keyctl_read(id: KeyringSerial, mut buffer: Option<Out<[u8]>>) -> Result<usize> {
    let capacity = buffer.as_mut().map_or(0, |b| b.len());
    unsafe {
        keyctl!(
            libc::KEYCTL_READ,
            id.get(),
            buffer.as_mut().map_or(ptr::null(), |b| b.as_mut_ptr()),
            capacity,
        )
    }
    .map(size)
}

pub fn keyctl_instantiate(
    id: KeyringSerial,
    payload: &[u8],
    ringid: Option<KeyringSerial>,
) -> Result<()> {
    unsafe {
        keyctl!(
            libc::KEYCTL_INSTANTIATE,
            id.get(),
            payload.as_ptr() as *const libc::c_void,
            payload.len(),
            opt_key_serial(ringid),
        )
    }
    .map(ignore)
}

pub fn keyctl_negate(
    id: KeyringSerial,
    timeout: TimeoutSeconds,
    ringid: Option<KeyringSerial>,
) -> Result<()> {
    unsafe {
        keyctl!(
            libc::KEYCTL_NEGATE,
            id.get(),
            timeout,
            opt_key_serial(ringid),
        )
    }
    .map(ignore)
}

pub fn keyctl_set_reqkey_keyring(reqkey_defl: DefaultKeyring) -> Result<DefaultKeyring> {
    unsafe { keyctl!(libc::KEYCTL_SET_REQKEY_KEYRING, reqkey_defl,) }.and_then(default_keyring)
}

pub fn keyctl_set_timeout(key: KeyringSerial, timeout: TimeoutSeconds) -> Result<()> {
    unsafe { keyctl!(libc::KEYCTL_SET_TIMEOUT, key.get(), timeout,) }.map(ignore)
}

pub fn keyctl_assume_authority(key: Option<KeyringSerial>) -> Result<()> {
    unsafe { keyctl!(libc::KEYCTL_ASSUME_AUTHORITY, opt_key_serial(key),) }.map(ignore)
}

pub fn keyctl_get_security(key: KeyringSerial, mut buffer: Option<Out<[u8]>>) -> Result<usize> {
    let capacity = buffer.as_mut().map_or(0, |b| b.len());
    unsafe {
        keyctl!(
            libc::KEYCTL_GET_SECURITY,
            key.get(),
            buffer.as_mut().map_or(ptr::null(), |b| b.as_mut_ptr()),
            capacity,
        )
    }
    .map(size)
}

pub fn keyctl_reject(
    id: KeyringSerial,
    timeout: TimeoutSeconds,
    error: errno::Errno,
    ringid: Option<KeyringSerial>,
) -> Result<()> {
    unsafe {
        keyctl!(
            libc::KEYCTL_REJECT,
            id.get(),
            timeout,
            error,
            opt_key_serial(ringid),
        )
    }
    .map(ignore)
}

pub fn keyctl_invalidate(id: KeyringSerial) -> Result<()> {
    unsafe { keyctl!(libc::KEYCTL_INVALIDATE, id.get(),) }.map(ignore)
}

pub fn keyctl_get_persistent(uid: libc::uid_t, id: KeyringSerial) -> Result<KeyringSerial> {
    unsafe { keyctl!(libc::KEYCTL_GET_PERSISTENT, uid, id.get(),) }.map(keyring_serial)
}

pub fn keyctl_session_to_parent() -> Result<()> {
    unsafe { keyctl!(libc::KEYCTL_SESSION_TO_PARENT,) }.map(ignore)
}

#[repr(C)]
struct DhComputeParamsKernel {
    priv_: i32,
    prime: i32,
    base: i32,
}

#[repr(C)]
struct DhKdfParamsKernel {
    hashname: *const libc::c_char,
    otherinfo: *const libc::c_void,
    otherinfolen: u32,
    _spare: [u32; 8],
}

pub fn keyctl_dh_compute(
    private: KeyringSerial,
    prime: KeyringSerial,
    base: KeyringSerial,
    mut buffer: Option<Out<[u8]>>,
) -> Result<usize> {
    let params = DhComputeParamsKernel {
        priv_: private.get(),
        prime: prime.get(),
        base: base.get(),
    };
    let capacity = buffer.as_mut().map_or(0, |b| b.len());
    unsafe {
        keyctl!(
            libc::KEYCTL_DH_COMPUTE,
            &params as *const DhComputeParamsKernel,
            buffer.as_mut().map_or(ptr::null(), |b| b.as_mut_ptr()),
            capacity,
            ptr::null() as *const DhKdfParamsKernel,
        )
    }
    .map(size)
}

pub fn keyctl_dh_compute_kdf(
    private: KeyringSerial,
    prime: KeyringSerial,
    base: KeyringSerial,
    hashname: &str,
    otherinfo: Option<&[u8]>,
    mut buffer: Option<Out<[u8]>>,
) -> Result<usize> {
    let params = DhComputeParamsKernel {
        priv_: private.get(),
        prime: prime.get(),
        base: base.get(),
    };
    let hash_cstr = cstring(hashname);
    let kdf_params = DhKdfParamsKernel {
        hashname: hash_cstr.as_ptr(),
        otherinfo: otherinfo.map_or(ptr::null(), |d| d.as_ptr()) as *const libc::c_void,
        otherinfolen: safe_len(otherinfo.map_or(0, |d| d.len()))?,
        _spare: [0; 8],
    };
    let capacity = buffer.as_mut().map_or(0, |b| b.len());
    unsafe {
        keyctl!(
            libc::KEYCTL_DH_COMPUTE,
            &params as *const DhComputeParamsKernel,
            buffer.as_mut().map_or(ptr::null(), |b| b.as_mut_ptr()),
            capacity,
            &kdf_params as *const DhKdfParamsKernel,
        )
    }
    .map(size)
}

pub enum Restriction<'a> {
    AllLinks,
    ByType {
        type_: &'a str,
        restriction: &'a str,
    },
}

pub fn keyctl_restrict_keyring(keyring: KeyringSerial, restriction: Restriction) -> Result<()> {
    let type_cstr;
    let restriction_cstr;

    let (type_ptr, restriction_ptr) = match restriction {
        Restriction::AllLinks => (ptr::null(), ptr::null()),
        Restriction::ByType {
            type_,
            restriction,
        } => {
            type_cstr = cstring(type_);
            restriction_cstr = cstring(restriction);

            (type_cstr.as_ptr(), restriction_cstr.as_ptr())
        },
    };
    unsafe {
        keyctl!(
            libc::KEYCTL_RESTRICT_KEYRING,
            keyring.get(),
            type_ptr,
            restriction_ptr,
        )
    }
    .map(ignore)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// #[non_exhaustive]
pub struct PKeyQuery {
    pub supported_ops: u32,
    pub key_size: u32,
    pub max_data_size: u16,
    pub max_sig_size: u16,
    pub max_enc_size: u16,
    pub max_dec_size: u16,
}

#[repr(C)]
pub struct PKeyQueryKernel {
    supported_ops: u32,
    key_size: u32,
    max_data_size: u16,
    max_sig_size: u16,
    max_enc_size: u16,
    max_dec_size: u16,
    _spare: [u32; 10],
}

impl PKeyQueryKernel {
    fn zeroed() -> Self {
        PKeyQueryKernel {
            supported_ops: 0,
            key_size: 0,
            max_data_size: 0,
            max_sig_size: 0,
            max_enc_size: 0,
            max_dec_size: 0,
            _spare: [0; 10],
        }
    }
}

impl From<PKeyQueryKernel> for PKeyQuery {
    fn from(kernel: PKeyQueryKernel) -> Self {
        PKeyQuery {
            supported_ops: kernel.supported_ops,
            key_size: kernel.key_size,
            max_data_size: kernel.max_data_size,
            max_sig_size: kernel.max_sig_size,
            max_enc_size: kernel.max_enc_size,
            max_dec_size: kernel.max_dec_size,
        }
    }
}

pub fn keyctl_pkey_query(key: KeyringSerial, info: &str) -> Result<PKeyQuery> {
    let mut query = PKeyQueryKernel::zeroed();
    let info_cstr = cstring(info);
    unsafe {
        keyctl!(
            libc::KEYCTL_PKEY_QUERY,
            key.get(),
            0,
            info_cstr.as_ptr(),
            &mut query as *mut PKeyQueryKernel,
        )
    }
    .map(ignore)?;

    Ok(query.into())
}

#[repr(C)]
struct PKeyOpParamsKernel {
    key_id: i32,
    in_len: u32,
    out_len: u32,
    in2_len: u32,
}

pub fn keyctl_pkey_encrypt(
    key: KeyringSerial,
    info: &str,
    data: &[u8],
    mut buffer: Out<[u8]>,
) -> Result<usize> {
    let params = PKeyOpParamsKernel {
        key_id: key.get(),
        in_len: safe_len(data.len())?,
        out_len: safe_len(buffer.len())?,
        in2_len: 0,
    };
    let info_cstr = cstring(info);
    unsafe {
        keyctl!(
            libc::KEYCTL_PKEY_ENCRYPT,
            &params as *const PKeyOpParamsKernel,
            info_cstr.as_ptr(),
            data.as_ptr(),
            buffer.as_mut_ptr(),
        )
    }
    .map(size)
}

pub fn keyctl_pkey_decrypt(
    key: KeyringSerial,
    info: &str,
    data: &[u8],
    mut buffer: Out<[u8]>,
) -> Result<usize> {
    let params = PKeyOpParamsKernel {
        key_id: key.get(),
        in_len: safe_len(data.len())?,
        out_len: safe_len(buffer.len())?,
        in2_len: 0,
    };
    let info_cstr = cstring(info);
    unsafe {
        keyctl!(
            libc::KEYCTL_PKEY_DECRYPT,
            &params as *const PKeyOpParamsKernel,
            info_cstr.as_ptr(),
            data.as_ptr(),
            buffer.as_mut_ptr(),
        )
    }
    .map(size)
}

pub fn keyctl_pkey_sign(
    key: KeyringSerial,
    info: &str,
    data: &[u8],
    mut buffer: Out<[u8]>,
) -> Result<usize> {
    let params = PKeyOpParamsKernel {
        key_id: key.get(),
        in_len: safe_len(data.len())?,
        out_len: safe_len(buffer.len())?,
        in2_len: 0,
    };
    let info_cstr = cstring(info);
    unsafe {
        keyctl!(
            libc::KEYCTL_PKEY_SIGN,
            &params as *const PKeyOpParamsKernel,
            info_cstr.as_ptr(),
            data.as_ptr(),
            buffer.as_mut_ptr(),
        )
    }
    .map(size)
}

pub fn keyctl_pkey_verify(key: KeyringSerial, info: &str, data: &[u8], sig: &[u8]) -> Result<bool> {
    let params = PKeyOpParamsKernel {
        key_id: key.get(),
        in_len: safe_len(data.len())?,
        out_len: 0,
        in2_len: safe_len(sig.len())?,
    };
    let info_cstr = cstring(info);
    unsafe {
        keyctl!(
            libc::KEYCTL_PKEY_VERIFY,
            &params as *const PKeyOpParamsKernel,
            info_cstr.as_ptr(),
            data.as_ptr(),
            sig.as_ptr(),
        )
    }
    .map(|res| res == 0)
}
