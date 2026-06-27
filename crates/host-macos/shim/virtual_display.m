// Minimal Objective-C shim over the *private* CoreGraphics CGVirtualDisplay API.
// Declares just enough of the reverse-engineered interface to create one virtual
// display and return its CGDirectDisplayID.
//
// The created display is retained in a file-global so it lives for the process
// lifetime; macOS tears it down when the process exits. Interface shape follows
// the widely-used reverse-engineering (e.g. Chromium's virtual_display_mac_util).
//
// NOTE: this creates a standard (non-HiDPI) display at the requested size. True
// Retina/HiDPI was attempted (settings.hiDPI + explicit CGDisplaySetDisplayMode)
// but a standalone virtual display doesn't reliably adopt a HiDPI mode without
// being mirrored to a physical display (the force-hidpi approach), which doesn't
// fit a capture workflow — so it's deferred. See docs/M4-hidpi-deferred.md.

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

// Create one virtual display at the given pixel size, labelled `name` (the
// connecting device — falls back to a default when null/empty). Retained until
// destroyed (or process exit). Returns its CGDirectDisplayID, or 0 on failure.
uint32_t extender_vdisplay_create(uint32_t width, uint32_t height, const char *name) {
    NSString *displayName = (name != NULL && name[0] != '\0')
        ? [NSString stringWithUTF8String:name]
        : nil;
    if (displayName == nil) {
        displayName = @"Universal Screens";
    }

    CGVirtualDisplayDescriptor *descriptor = [[CGVirtualDisplayDescriptor alloc] init];
    descriptor.queue = dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0);
    descriptor.name = displayName;
    descriptor.maxPixelsWide = width;
    descriptor.maxPixelsHigh = height;
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
    settings.hiDPI = 0;
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
