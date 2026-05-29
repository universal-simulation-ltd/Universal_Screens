// Minimal Objective-C shim over the *private* CoreGraphics CGVirtualDisplay API,
// for the M3 feasibility spike. Declares just enough of the reverse-engineered
// interface to create one virtual display and return its CGDirectDisplayID.
//
// The created display is retained in a file-global so it lives for the process
// lifetime; macOS tears it down when the process exits. Interface shape follows
// the widely-used reverse-engineering (e.g. Chromium's virtual_display_mac_util).

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

static CGVirtualDisplay *g_display = nil;

// Create one virtual display of the given pixel size. Returns its
// CGDirectDisplayID, or 0 on failure. Retained for the process lifetime.
uint32_t extender_vdisplay_create(uint32_t width, uint32_t height, uint32_t hidpi) {
    CGVirtualDisplayDescriptor *descriptor = [[CGVirtualDisplayDescriptor alloc] init];
    descriptor.queue = dispatch_get_global_queue(DISPATCH_QUEUE_PRIORITY_HIGH, 0);
    descriptor.name = @"ExtenderScreen Virtual Display";
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
    settings.hiDPI = hidpi;
    settings.modes = @[ mode ];

    if (![display applySettings:settings]) {
        return 0;
    }

    g_display = display; // retained (ARC strong global) for the process lifetime
    return display.displayID;
}
