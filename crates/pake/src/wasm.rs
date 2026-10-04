//! JavaScript bindings (`--features wasm`), for a browser tab that pairs by
//! itself — the same Rust as every native peer, so the two cannot drift.
//!
//! ```js
//! import init, { PakeSession } from "./unisim_pake.js";
//! await init();
//! const me = new PakeSession(true, new TextEncoder().encode("4821"), ctx);
//! send(me.message);                       // 33 bytes
//! const key = me.finish(await receive()); // a PakeKey
//! send(key.confirmation(true));           // prove it, initiator first
//! if (!key.verify(false, await receive())) throw new Error("wrong code");
//! const aesKey = key.derive(new TextEncoder().encode("my-app/aes"));
//! ```

use wasm_bindgen::prelude::*;

use crate::{normalise_code, Pake, Role, SharedKey};

fn role(initiator: bool) -> Role {
    if initiator {
        Role::Initiator
    } else {
        Role::Responder
    }
}

/// One side of an exchange in progress.
#[wasm_bindgen]
pub struct PakeSession {
    pake: Option<Pake>,
    message: Vec<u8>,
}

#[wasm_bindgen]
impl PakeSession {
    /// Start as the initiator (`true`: the device the code is typed into) or
    /// the responder (`false`: the device showing it).
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(initiator: bool, code: &[u8], context: &[u8]) -> PakeSession {
        let (pake, message) = Pake::start(role(initiator), code, context);
        PakeSession { pake: Some(pake), message }
    }

    /// The bytes to send to the peer.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn message(&self) -> Vec<u8> {
        self.message.clone()
    }

    /// Finish with the peer's message. ⚠️ A wrong code still finishes — with a
    /// different key. Confirm before trusting it.
    ///
    /// # Errors
    /// A malformed message, or a session that has already finished.
    pub fn finish(&mut self, peer_message: &[u8]) -> Result<PakeKey, String> {
        let pake = self.pake.take().ok_or_else(|| "this pairing has already finished".to_owned())?;
        pake.finish(peer_message).map(|key| PakeKey { key }).map_err(|e| e.to_string())
    }
}

/// The shared key. Equal on both sides only if the codes matched.
#[wasm_bindgen]
pub struct PakeKey {
    key: SharedKey,
}

#[wasm_bindgen]
impl PakeKey {
    /// A 32-byte key for one purpose, named by `label`.
    #[must_use]
    pub fn derive(&self, label: &[u8]) -> Vec<u8> {
        self.key.derive(label).to_vec()
    }

    /// The tag this side (`initiator` = which role we play) sends as proof.
    #[must_use]
    pub fn confirmation(&self, initiator: bool) -> Vec<u8> {
        self.key.confirmation(role(initiator)).to_vec()
    }

    /// Check the peer's tag. `peer_initiator` is the PEER's role.
    #[must_use]
    pub fn verify(&self, peer_initiator: bool, tag: &[u8]) -> bool {
        self.key.verify_confirmation(role(peer_initiator), tag)
    }
}

/// [`normalise_code`] for JS: what to pass as `code` for a typed string.
#[wasm_bindgen(js_name = normaliseCode)]
#[must_use]
pub fn normalise_code_js(typed: &str) -> Vec<u8> {
    normalise_code(typed)
}
