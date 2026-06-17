package com.universalsim.extender

import com.journeyapps.barcodescanner.CaptureActivity

/**
 * zxing-android-embedded's built-in capture screen is locked to landscape (its
 * manifest entry forces it), so even `setOrientationLocked(false)` opens sideways.
 * This empty subclass is declared `android:screenOrientation="portrait"` in our
 * manifest and pointed at via `ScanOptions.setCaptureActivity(...)`, so the
 * scanner opens upright.
 */
class PortraitCaptureActivity : CaptureActivity()
