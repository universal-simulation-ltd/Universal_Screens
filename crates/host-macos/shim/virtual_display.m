// Minimal Objective-C shim over the *private* CoreGraphics CGVirtualDisplay API.
// Declares just enough of the reverse-engineered interface to create one virtual
// display and return its CGDirectDisplayID.
//
// The created display is retained in a file-global so it lives for the process
// lifetime; macOS tears it down when the process exits. Interface shape follows
// the widely-used reverse-engineering (e.g. Chromium's virtual_display_mac_util).
//
// NOTE: at 100% scaling this creates a standard display at the requested size.
// When the client asks for scaling (150% etc.) it is HiDPI: the mode is the
// point size and the maximum pixel size twice that, which macOS 27 adopts as the
// current mode straight away (it didn't on 26.2 with other shapes; see
// docs/M4-hidpi-deferred.md, 2026-11-18 update).

#include <stdbool.h>
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>

// ---- private CoreGraphics interfaces (reverse-engineered) ----

@interface CGVirtualDisplayDescriptor : NSObject
@property(retain) dispatch_queue_t queue;
@property(copy) NSString *name;
@property uint32_t maxPixelsWide;
@property uint32_t maxPixelsHigh;
@property CGSize sizeInMillimeters;
@property uint32_t productID;
@property uint32_t vendorID;
@property uint32_t serialNum;
@property CGPoint redPrimary;
@property CGPoint greenPrimary;
@property CGPoint bluePrimary;
@property CGPoint whitePoint;
@end

@interface CGVirtualDisplayMode : NSObject
- (instancetype)initWithWidth:(uint32_t)width
                       height:(uint32_t)height
                  refreshRate:(double)refreshRate;
@end

@interface CGVirtualDisplaySettings : NSObject
@property uint32_t hiDPI;
@property(retain) NSArray *modes;
@end

@interface CGVirtualDisplay : NSObject
- (instancetype)initWithDescriptor:(CGVirtualDisplayDescriptor *)descriptor;
- (BOOL)applySettings:(CGVirtualDisplaySettings *)settings;
@property(readonly) uint32_t displayID;
@end

// ---- shim ----

// Created displays are retained here keyed by their CGDirectDisplayID, so several
// can coexist and each can be torn down individually (releasing the object lets
// macOS remove the display). Guarded by @synchronized since create runs on the
// server thread and destroy can be called from the GUI thread.
static NSMutableDictionary<NSNumber *, CGVirtualDisplay *> *g_displays = nil;

static NSMutableDictionary *displaysTable(void) {
    if (g_displays == nil) {
        g_displays = [[NSMutableDictionary alloc] init];
    }
    return g_displays;
}

// Release a previously-created virtual display so the window server tears it down.
// Returns 1 if a display with that id was held (and is now removed), else 0.
uint32_t extender_vdisplay_destroy(uint32_t displayID) {
    @synchronized(displaysTable()) {
        NSNumber *key = @(displayID);
        if (displaysTable()[key] != nil) {
            [displaysTable() removeObjectForKey:key]; // ARC releases → display removed
            return 1;
        }
    }
    return 0;
}

// Create one virtual display of `width`×`height` points, labelled `name` (the
// connecting device — falls back to a default when null/empty). With `hidpi`
// it is Retina: a framebuffer twice the point size each way, which is what makes
// a client's 150% scaling crisp. Retained until destroyed (or process exit).
// Returns its CGDirectDisplayID, or 0 on failure.
//
// HiDPI note: the maximum pixel size must be the doubled size, or macOS keeps
// the display at 1x (docs/M4-hidpi-deferred.md; works on macOS 27).
uint32_t extender_vdisplay_create(uint32_t width, uint32_t height, const char *name, bool hidpi) {
    NSString *displayName = (name != NULL && name[0] != '\0')
        ? [NSString stringWithUTF8String:name]
        : nil;
    if (displayName == nil) {
        displayName = @"Universal Screens";
    }

    CGVirtualDisplayDescriptor *descriptor = [[CGVirtualDisplayDescriptor alloc] init];
    descriptor.queue = dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0);
    descriptor.name = displayName;
    uint32_t factor = hidpi ? 2 : 1;
    descriptor.maxPixelsWide = width * factor;
    descriptor.maxPixelsHigh = height * factor;
    descriptor.sizeInMillimeters = CGSizeMake(25.4 * width / 109.0, 25.4 * height / 109.0);
    descriptor.productID = 0x1234;
    descriptor.vendorID = 0x3456;
    descriptor.serialNum = 0x0001;
    descriptor.whitePoint = CGPointMake(0.3125, 0.3291);
    descriptor.bluePrimary = CGPointMake(0.1494, 0.0557);
    descriptor.greenPrimary = CGPointMake(0.2559, 0.6983);
    descriptor.redPrimary = CGPointMake(0.6797, 0.3203);

    CGVirtualDisplay *display = [[CGVirtualDisplay alloc] initWithDescriptor:descriptor];
    if (display == nil) {
        return 0;
    }

    CGVirtualDisplayMode *mode = [[CGVirtualDisplayMode alloc] initWithWidth:width
                                                                      height:height
                                                                 refreshRate:60.0];
    CGVirtualDisplaySettings *settings = [[CGVirtualDisplaySettings alloc] init];
    settings.hiDPI = hidpi ? 1 : 0;
    settings.modes = @[ mode ];

    if (![display applySettings:settings]) {
        return 0;
    }

    uint32_t did = display.displayID;
    @synchronized(displaysTable()) {
        displaysTable()[@(did)] = display; // retained by the table until destroyed
    }
    return did;
}
