/*
 * extender_ffi.h — C ABI for the ExtenderScreen mobile client core.
 *
 * Links against the `extender-mobile-ffi` crate (staticlib for iOS, cdylib for
 * Android). A native shell connects, pulls encoded frames, and pushes input;
 * decoding is done on the platform (VideoToolbox / MediaCodec).
 *
 * All byte buffers handed out are Annex-B (start-code-delimited NAL units): a
 * Start event carries the parameter sets (SPS/PPS), each Frame event carries the
 * frame's NALs. On a keyframe, prepend the stored parameter sets before decode.
 *
 * Threading: call extender_session_next_event from a single consumer thread; the
 * extender_send_* calls may come from any thread. Free every non-NULL event with
 * extender_event_free and the session with extender_session_free.
 */
#ifndef EXTENDER_FFI_H
#define EXTENDER_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ExtenderSession ExtenderSession;
typedef struct ExtenderEvent ExtenderEvent;

typedef enum {
  EXTENDER_EVENT_START = 0, /* width/height/codec valid; data = Annex-B param sets */
  EXTENDER_EVENT_FRAME = 1, /* pts/keyframe/data valid; data = Annex-B NAL units   */
} ExtenderEventKind;

typedef enum {
  EXTENDER_TOUCH_BEGAN = 0,
  EXTENDER_TOUCH_MOVED = 1,
  EXTENDER_TOUCH_ENDED = 2,
  EXTENDER_TOUCH_CANCELLED = 3,
} ExtenderTouchPhase;

typedef enum {
  EXTENDER_MOUSE_LEFT = 0,
  EXTENDER_MOUSE_RIGHT = 1,
  EXTENDER_MOUSE_MIDDLE = 2,
} ExtenderMouseButton;

/* --- session lifecycle --- */

/* Connect to "host:port"; mirror=true requests the host's primary display
 * (remote control) instead of a virtual second screen. NULL on failure. */
ExtenderSession *extender_session_connect(const char *addr, uint32_t width,
                                          uint32_t height, bool mirror);

/* Block for the next event; NULL when the stream ends. Free with
 * extender_event_free. */
ExtenderEvent *extender_session_next_event(ExtenderSession *session);

void extender_session_free(ExtenderSession *session);

/* --- event accessors --- */

ExtenderEventKind extender_event_kind(const ExtenderEvent *event);
uint32_t extender_event_width(const ExtenderEvent *event);
uint32_t extender_event_height(const ExtenderEvent *event);
uint32_t extender_event_codec(const ExtenderEvent *event); /* 0 = H.264, 1 = HEVC */
bool extender_event_keyframe(const ExtenderEvent *event);
int64_t extender_event_pts_value(const ExtenderEvent *event);
int32_t extender_event_pts_timescale(const ExtenderEvent *event);
/* Annex-B bytes; writes length to *len. Valid until extender_event_free. */
const uint8_t *extender_event_data(const ExtenderEvent *event, size_t *len);
void extender_event_free(ExtenderEvent *event);

/* --- upstream input (x/y normalized [0,1] from top-left) --- */

void extender_send_mouse_move(ExtenderSession *session, float x, float y);
void extender_send_mouse_button(ExtenderSession *session,
                                ExtenderMouseButton button, bool pressed);
void extender_send_scroll(ExtenderSession *session, float dx, float dy);
void extender_send_touch(ExtenderSession *session, uint32_t id,
                         ExtenderTouchPhase phase, float x, float y);
void extender_send_secondary_click(ExtenderSession *session, float x, float y);
void extender_send_pinch(ExtenderSession *session, float scale);
void extender_send_text(ExtenderSession *session, const char *text);

/* Key by USB-HID keyboard usage id; pressed = down/up (send down then up for a
 * tap). Common presentation-clicker keys are defined below. */
void extender_send_key(ExtenderSession *session, uint32_t hid_code, bool pressed);

#define EXTENDER_KEY_PAGE_UP 0x4B   /* previous slide */
#define EXTENDER_KEY_PAGE_DOWN 0x4E /* next slide     */
#define EXTENDER_KEY_LEFT 0x50      /* previous slide (arrow) */
#define EXTENDER_KEY_RIGHT 0x4F     /* next slide (arrow)     */
#define EXTENDER_KEY_HOME 0x4A      /* first slide */
#define EXTENDER_KEY_END 0x4D       /* last slide  */
#define EXTENDER_KEY_ESCAPE 0x29    /* end slideshow */
#define EXTENDER_KEY_F5 0x3E        /* start slideshow (PowerPoint) */
#define EXTENDER_KEY_B 0x05         /* blank (PowerPoint) */
#define EXTENDER_KEY_PERIOD 0x37    /* blank (Keynote/Google Slides) */

#ifdef __cplusplus
}
#endif

#endif /* EXTENDER_FFI_H */
