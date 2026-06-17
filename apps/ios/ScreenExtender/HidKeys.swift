import Foundation

/// USB-HID keyboard usage ids for the clicker, mirroring the `EXTENDER_KEY_*`
/// defines in `extender_ffi.h` and the host's keymap.
enum HidKeys {
    static let pageUp: UInt32 = 0x4B // previous slide
    static let pageDown: UInt32 = 0x4E // next slide
    static let home: UInt32 = 0x4A // first slide
    static let end: UInt32 = 0x4D // last slide
    static let escape: UInt32 = 0x29 // end slideshow
    static let f5: UInt32 = 0x3E // start slideshow (PowerPoint)
    static let b: UInt32 = 0x05 // blank (PowerPoint)
    static let period: UInt32 = 0x37 // blank (Keynote / Google Slides)
}
