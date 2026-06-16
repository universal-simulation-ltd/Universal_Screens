package com.universalsim.extender

/** USB-HID keyboard usage ids, matching the host's `hid_to_macos` map. */
object HidKeys {
    const val ARROW_RIGHT = 0x4F // next slide
    const val ARROW_LEFT = 0x50  // previous slide
    const val PAGE_DOWN = 0x4E   // next slide (what physical clickers send)
    const val PAGE_UP = 0x4B     // previous slide
    const val HOME = 0x4A        // first slide
    const val END = 0x4D         // last slide
    const val ESCAPE = 0x29      // end slideshow
    const val F5 = 0x3E          // start slideshow (PowerPoint)
    const val PERIOD = 0x37      // blank (Keynote / Google Slides)
    const val B = 0x05           // blank (PowerPoint)
}

/** Touch phases, matching the protocol's `TouchPhase`. */
object TouchPhase {
    const val BEGAN = 0
    const val MOVED = 1
    const val ENDED = 2
    const val CANCELLED = 3
}
