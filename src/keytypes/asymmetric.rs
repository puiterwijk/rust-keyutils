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

//! Asymmetric keys

use std::borrow::Cow;

use crate::keytype::*;
use crate::{Key, Keyring, KeyringSerial};

/// Asymmetric keys support encrypting, decrypting, signing, and verifying data.
///
/// Note that when searching for an asymmetric key, the following formats may be used:
///
///   - `ex:<id>`: an exact match of the key ID
///   - `id:<id>`: a partial match of the key ID
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Asymmetric;

impl KeyType for Asymmetric {
    /// Asymmetric key descriptions may be left empty to allow the kernel to generate a random
    /// description.
    type Description = str;
    /// Asymmetric key payloads must be in a format supported by the kernel.
    ///
    ///   - `pkcs8`
    ///   - `tpm`
    ///   - `x509`
    ///
    /// The kernel will automatically detect the format.
    type Payload = [u8];

    fn name() -> &'static str {
        "asymmetric"
    }
}

/// A restriction that may be placed onto a keyring using an asymmetric key.
#[derive(Debug, Clone, PartialEq, Eq)]
// #[non_exhaustive]
pub enum AsymmetricRestriction {
    /// Only allow keys which have been signed by a key on the builtin trusted keyring.
    BuiltinTrusted,
    /// Only allow keys which have been signed by a key on the builtin or secondary trusted
    /// keyrings.
    BuiltinAndSecondaryTrusted,
    /// Only allow keys which have been signed by the given key.
    Key {
        /// The signing key.
        key: Key,
        /// Whether or not chaining should be used (see `Chained`).
        chained: bool,
    },
    /// Only allow keys which have been signed by a key on the given keyring.
    Keyring {
        /// The keyring with permitted signing keys.
        keyring: Keyring,
        /// Whether or not chaining should be used (see `Chained`).
        chained: bool,
    },
    /// When chaining the destination keyring is also searched for signing keys.
    ///
    /// This allows building up a chain of trust in the destination keyring.
    Chained,
}

impl AsymmetricRestriction {
    fn restriction_str(id: KeyringSerial, chained: bool) -> String {
        let chain_suffix = if chained { ":chain" } else { "" };
        format!("key_or_keyring:{}{}", id, chain_suffix)
    }
}

impl KeyRestriction for AsymmetricRestriction {
    fn restriction(&self) -> Cow<str> {
        match self {
            AsymmetricRestriction::BuiltinTrusted => "builtin_trusted".into(),
            AsymmetricRestriction::BuiltinAndSecondaryTrusted => {
                "builtin_and_secondary_trusted".into()
            },
            AsymmetricRestriction::Key {
                key,
                chained,
            } => Self::restriction_str(key.serial(), *chained).into(),
            AsymmetricRestriction::Keyring {
                keyring,
                chained,
            } => Self::restriction_str(keyring.serial(), *chained).into(),
            AsymmetricRestriction::Chained => "key_or_keyring:0:chain".into(),
        }
    }
}

impl RestrictableKeyType for Asymmetric {
    type Restriction = AsymmetricRestriction;
}
