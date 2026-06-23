// Input capture → encoded `Input` events (M7d). Mouse, physical keyboard (HID),
// wheel, touch (tap/drag), pinch + long-press gestures, pointer-lock relative
// mode, and IME committed text. Coordinates are frame-normalized [0,1] (the host
// maps them onto the display), matching the mobile apps.
import { protocol } from "./wasm.js";
import { hidFor } from "./hid.js";

const DOM_BUTTON = { 0: 0, 1: 2, 2: 1 }; // DOM event.button → protocol Button (L/M/R)
const LONG_PRESS_MS = 500;
const MOVE_SLOP = 12; // px before a touch counts as a drag (vs. a tap/long-press)
// Protocol TouchPhase codes.
const PHASE = { Began: 0, Moved: 1, Ended: 2, Cancelled: 3 };

export class InputController {
  constructor(transport, renderer, canvas) {
    this.t = transport;
    this.r = renderer;
    this.canvas = canvas;
    this.enabled = false; // forward keyboard/clicker keys
    this.pointerInput = false; // forward pointer/touch (modes that stream video)
    this.locked = false; // pointer-lock relative mode
    // Single-touch gesture state.
    this.touch = null; // { id, startX, startY, dragging, longPressed, timer }
    this.pinch = null; // { startDist }
    this._listeners = [];
    this._textEl = null;
  }

  setMode({ enabled, pointerInput }) {
    this.enabled = enabled;
    this.pointerInput = pointerInput;
  }

  // --- lifecycle -----------------------------------------------------------

  attach() {
    const c = this.canvas;
    this._on(c, "pointermove", (e) => this._onPointerMove(e));
    this._on(c, "pointerdown", (e) => this._onPointerDown(e));
    this._on(c, "pointerup", (e) => this._onPointerUp(e));
    this._on(c, "contextmenu", (e) => e.preventDefault());
    this._on(c, "wheel", (e) => this._onWheel(e), { passive: false });
    this._on(c, "touchstart", (e) => this._onTouchStart(e), { passive: false });
    this._on(c, "touchmove", (e) => this._onTouchMove(e), { passive: false });
    this._on(c, "touchend", (e) => this._onTouchEnd(e), { passive: false });
    this._on(c, "touchcancel", (e) => this._onTouchEnd(e), { passive: false });
    this._on(window, "keydown", (e) => this._onKey(e, true));
    this._on(window, "keyup", (e) => this._onKey(e, false));
    this._on(document, "pointerlockchange", () => {
      this.locked = document.pointerLockElement === this.canvas;
    });
    // Hidden field to receive IME composition (soft keyboards / dead keys).
    const input = document.createElement("input");
    input.setAttribute("aria-hidden", "true");
    input.style.cssText = "position:fixed;opacity:0;pointer-events:none;left:-9999px;";
    document.body.appendChild(input);
    this._on(input, "compositionend", (e) => {
      if (this.enabled && e.data) this.t.send(protocol.encode_text(e.data));
    });
    this._textEl = input;
  }

  detach() {
    for (const [el, type, fn, opts] of this._listeners) el.removeEventListener(type, fn, opts);
    this._listeners = [];
    this._textEl?.remove();
    this._textEl = null;
    if (this.locked) document.exitPointerLock?.();
  }

  /// Enter pointer-lock relative mode (control mode "grab cursor").
  lockPointer() {
    if (this.pointerInput) this.canvas.requestPointerLock?.();
  }

  _on(el, type, fn, opts) {
    el.addEventListener(type, fn, opts);
    this._listeners.push([el, type, fn, opts]);
  }

  // --- mouse / pen ---------------------------------------------------------

  _onPointerMove(e) {
    if (!this.pointerInput || e.pointerType === "touch") return;
    if (this.locked) {
      this.t.send(protocol.encode_mouse_move_relative(e.movementX, e.movementY));
    } else {
      const p = this.r.normalize(e.clientX, e.clientY);
      if (p) this.t.send(protocol.encode_mouse_move(p.x, p.y));
    }
  }

  _onPointerDown(e) {
    if (!this.pointerInput || e.pointerType === "touch") return;
    if (!this.locked) {
      const p = this.r.normalize(e.clientX, e.clientY);
      if (p) this.t.send(protocol.encode_mouse_move(p.x, p.y));
    }
    this.t.send(protocol.encode_mouse_button(DOM_BUTTON[e.button] ?? 0, true));
  }

  _onPointerUp(e) {
    if (!this.pointerInput || e.pointerType === "touch") return;
    this.t.send(protocol.encode_mouse_button(DOM_BUTTON[e.button] ?? 0, false));
  }

  _onWheel(e) {
    if (!this.pointerInput) return;
    e.preventDefault();
    // deltaY>0 is scroll-down in the browser; the protocol's +dy is scroll-up.
    const k = e.deltaMode === 0 ? 1 / 40 : 1; // pixels → ~lines
    this.t.send(protocol.encode_scroll(-e.deltaX * k, -e.deltaY * k));
  }

  // --- keyboard ------------------------------------------------------------

  _onKey(e, pressed) {
    if (!this.enabled || e.isComposing) return; // IME commits go via Text
    const hid = hidFor(e.code);
    if (hid === undefined) return;
    e.preventDefault();
    this.t.send(protocol.encode_key(hid, pressed));
  }

  // --- touch + gestures ----------------------------------------------------

  _sendTouch(id, phase, clientX, clientY) {
    const p = this.r.normalize(clientX, clientY);
    if (p) this.t.send(protocol.encode_touch(id, phase, p.x, p.y));
  }

  _onTouchStart(e) {
    if (!this.pointerInput) return;
    e.preventDefault();
    if (e.touches.length >= 2) {
      // Two fingers → pinch; abandon any single-touch sequence.
      this._cancelTouch();
      this.pinch = { startDist: this._dist(e.touches[0], e.touches[1]) };
      return;
    }
    const t = e.changedTouches[0];
    const self = this;
    this.touch = {
      id: t.identifier >>> 0,
      startX: t.clientX,
      startY: t.clientY,
      dragging: false,
      longPressed: false,
      timer: setTimeout(() => {
        // Held still → secondary (right) click.
        if (self.touch && !self.touch.dragging) {
          self.touch.longPressed = true;
          const p = self.r.normalize(self.touch.startX, self.touch.startY);
          if (p) self.t.send(protocol.encode_secondary_click(p.x, p.y));
        }
      }, LONG_PRESS_MS),
    };
  }

  _onTouchMove(e) {
    if (!this.pointerInput) return;
    e.preventDefault();
    if (this.pinch && e.touches.length >= 2) {
      const dist = this._dist(e.touches[0], e.touches[1]);
      if (this.pinch.startDist > 0) {
        this.t.send(protocol.encode_pinch(dist / this.pinch.startDist));
      }
      return;
    }
    if (!this.touch) return;
    const t = [...e.changedTouches].find((x) => (x.identifier >>> 0) === this.touch.id);
    if (!t) return;
    const moved = Math.hypot(t.clientX - this.touch.startX, t.clientY - this.touch.startY);
    if (!this.touch.dragging && moved > MOVE_SLOP && !this.touch.longPressed) {
      clearTimeout(this.touch.timer);
      this.touch.dragging = true;
      this._sendTouch(this.touch.id, PHASE.Began, this.touch.startX, this.touch.startY);
    }
    if (this.touch.dragging) this._sendTouch(this.touch.id, PHASE.Moved, t.clientX, t.clientY);
  }

  _onTouchEnd(e) {
    if (!this.pointerInput) return;
    if (this.pinch) {
      if (e.touches.length < 2) this.pinch = null;
      return;
    }
    if (!this.touch) return;
    clearTimeout(this.touch.timer);
    const t = [...e.changedTouches].find((x) => (x.identifier >>> 0) === this.touch.id) ?? e.changedTouches[0];
    if (this.touch.dragging) {
      this._sendTouch(this.touch.id, PHASE.Ended, t.clientX, t.clientY);
    } else if (!this.touch.longPressed) {
      // A tap → quick began+ended (the host turns it into a left click).
      this._sendTouch(this.touch.id, PHASE.Began, this.touch.startX, this.touch.startY);
      this._sendTouch(this.touch.id, PHASE.Ended, this.touch.startX, this.touch.startY);
    }
    this.touch = null;
  }

  _cancelTouch() {
    if (this.touch) {
      clearTimeout(this.touch.timer);
      if (this.touch.dragging) this._sendTouch(this.touch.id, PHASE.Cancelled, this.touch.startX, this.touch.startY);
      this.touch = null;
    }
  }

  _dist(a, b) {
    return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
  }
}
