// Draws decoded VideoFrames to a canvas, letterboxed to preserve aspect ratio,
// and maps client (mouse/touch) coordinates back to the frame-normalized [0,1]
// space the host expects (independent of canvas size / letterbox bars).
export class CanvasRenderer {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d", { alpha: false });
    // The destination rect of the last drawn frame, for coordinate mapping.
    this.rect = null; // { dx, dy, dw, dh }
  }

  /// Draw a VideoFrame (and close it). Sizes the canvas backing store to the
  /// frame on first sight so the image is pixel-exact.
  draw(frame) {
    const fw = frame.displayWidth;
    const fh = frame.displayHeight;
    const cw = this.canvas.clientWidth || fw;
    const ch = this.canvas.clientHeight || fh;
    if (this.canvas.width !== cw || this.canvas.height !== ch) {
      this.canvas.width = cw;
      this.canvas.height = ch;
    }
    const scale = Math.min(cw / fw, ch / fh);
    const dw = fw * scale;
    const dh = fh * scale;
    const dx = (cw - dw) / 2;
    const dy = (ch - dh) / 2;
    this.ctx.fillStyle = "#000";
    this.ctx.fillRect(0, 0, cw, ch);
    this.ctx.drawImage(frame, dx, dy, dw, dh);
    this.rect = { dx, dy, dw, dh };
    frame.close();
  }

  /// Map a pointer event to frame-normalized coords, or null if outside the
  /// drawn image (the letterbox bars).
  normalize(clientX, clientY) {
    if (!this.rect) return null;
    const box = this.canvas.getBoundingClientRect();
    const x = (clientX - box.left - this.rect.dx) / this.rect.dw;
    const y = (clientY - box.top - this.rect.dy) / this.rect.dh;
    if (x < 0 || x > 1 || y < 0 || y > 1) return null;
    return { x, y };
  }
}
