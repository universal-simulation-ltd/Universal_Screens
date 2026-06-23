// Loads the protocol WASM shim (crates/protocol-wasm, built into ../pkg) and
// re-exports its functions as `protocol`. Call `ready()` once before use.
import init, * as protocol from "../pkg/extender_protocol.js";

let readyPromise = null;

/// Initialise the WASM module (idempotent). In the browser this fetches
/// ../pkg/extender_protocol_bg.wasm relative to this module's URL.
export function ready() {
  return (readyPromise ??= init());
}

export { protocol };
