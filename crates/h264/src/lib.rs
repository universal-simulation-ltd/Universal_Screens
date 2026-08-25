//! The part of screen mirroring that is not about an operating system: how a
//! captured BGRA frame is sized for the encoder, and how openh264's Annex-B
//! output becomes the protocol's wire form.
//!
//! Every host that mirrors a screen needs exactly this, identically — the wire
//! format is the *client's* contract, so a host that framed it differently would
//! be a host whose stream no client could play. It lived in
//! `host-windows/src/stream.rs` while there was one host to serve; the Linux
//! host (Stage 2b of `docs/LINUX-HOST.md`) is the second, and a second copy of a
//! bit-level format is the kind of duplication that ends with two subtly
//! different answers and no way to tell which is right.
//!
//! ⚠️ What deliberately stays behind in each host's own `stream.rs`: capturing
//! the screen, enumerating monitors, and the encoder settings. Those are the
//! platform, and pretending otherwise is what makes a "shared" crate grow
//! `cfg(target_os)` blocks.
//!
//! No `cfg` in here, and nothing linked — so this crate compiles on Windows,
//! macOS and Linux alike, and its tests run anywhere.

/// The dimensions to *encode* at: the source, capped so the long side is at most
/// `max_long` (keeping aspect, rounded down to even — H.264 needs even
/// dimensions). A phone doesn't need a full 1080p+ desktop, and a smaller frame
/// keeps the software encoder real-time.
#[must_use]
pub fn encode_dims(w: u32, h: u32, max_long: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max_long {
        return (w, h);
    }
    let s = f64::from(max_long) / f64::from(long);
    let nw = ((f64::from(w) * s) as u32 & !1).max(2);
    let nh = ((f64::from(h) * s) as u32 & !1).max(2);
    (nw, nh)
}

/// Downscale a tightly-packed BGRA buffer from `sw`×`sh` to `dw`×`dh`. Channel
/// order is irrelevant to a per-channel resize, so the BGRA layout is preserved.
///
/// Returns a copy of `src` unchanged if it is too short to describe `sw`×`sh` —
/// a wrong-sized frame is better than a panic in a capture loop.
#[must_use]
pub fn downscale(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    match image::ImageBuffer::<image::Rgba<u8>, &[u8]>::from_raw(sw, sh, src) {
        Some(img) => {
            image::imageops::resize(&img, dw, dh, image::imageops::FilterType::Triangle).into_raw()
        }
        None => src.to_vec(),
    }
}

/// Copy the first `width*4` bytes of the first `height` rows out of a tightly-
/// packed BGRA buffer (cropping odd edges down to the even dimensions).
///
/// Reuses `out`'s allocation, because this runs 30 times a second.
pub fn pack_rows(src: &[u8], src_w: u32, width: u32, height: u32, out: &mut Vec<u8>) {
    let row = (width * 4) as usize;
    let src_row = (src_w * 4) as usize;
    out.clear();
    out.reserve(row * height as usize);
    for y in 0..height as usize {
        let s = y * src_row;
        let Some(chunk) = src.get(s..s + row) else { return };
        out.extend_from_slice(chunk);
    }
}

/// The two reusable buffers a frame loop needs, so a 30 fps stream isn't
/// allocating a desktop's worth of pixels twice per frame.
///
/// Owned by the caller and kept across iterations; [`Scratch::fit`] is the whole
/// per-frame path from "what capture handed back" to "what goes into the YUV
/// converter", which is the part both hosts had written out longhand.
#[derive(Default)]
pub struct Scratch {
    /// The captured frame cropped to even dimensions, tightly packed.
    packed: Vec<u8>,
    /// The downscaled copy — untouched, and never allocated, when the capture is
    /// already at the encode size.
    scaled: Vec<u8>,
}

impl Scratch {
    /// A fresh pair of empty buffers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Crop `bgra` (whose rows are `src_w` pixels wide) to `crop`, downscale it
    /// to `out` if those differ, and return the result. The slice borrows `self`
    /// until the next call.
    pub fn fit(&mut self, bgra: &[u8], src_w: u32, crop: (u32, u32), out: (u32, u32)) -> &[u8] {
        pack_rows(bgra, src_w, crop.0, crop.1, &mut self.packed);
        if out == crop {
            return &self.packed;
        }
        self.scaled = downscale(&self.packed, crop.0, crop.1, out.0, out.1);
        &self.scaled
    }
}

/// One encoder output, split into the protocol's wire form.
pub struct Split {
    /// SPS (NAL type 7) and PPS (type 8), **raw** — no start code, no length
    /// prefix. These go in `Message::StreamStart`'s `parameter_sets`.
    pub parameter_sets: Vec<Vec<u8>>,
    /// Every other NAL, concatenated as **AVCC**: each one 4-byte big-endian
    /// length-prefixed. This is `Message::Frame`'s `data`.
    pub frame_data: Vec<u8>,
    /// True when an IDR slice (type 5) is present, i.e. this frame is a
    /// keyframe a client can start decoding from.
    pub keyframe: bool,
}

/// Split an Annex-B bitstream into the protocol's wire form.
///
/// ⚠️ The two halves are framed **differently on purpose**: parameter sets raw,
/// frame data AVCC. That is what the macOS host's VideoToolbox path emits
/// natively, so it is what every shipped client already decodes — changing
/// either side here breaks playback on clients that are already in people's
/// pockets.
#[must_use]
pub fn split_annex_b(data: &[u8]) -> Split {
    let mut parameter_sets = Vec::new();
    let mut frame_data = Vec::new();
    let mut keyframe = false;
    for nal in annex_b_nals(data) {
        let Some(&first) = nal.first() else { continue };
        match first & 0x1F {
            7 | 8 => parameter_sets.push(nal.to_vec()),
            t => {
                if t == 5 {
                    keyframe = true;
                }
                frame_data.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                frame_data.extend_from_slice(nal);
            }
        }
    }
    Split { parameter_sets, frame_data, keyframe }
}

/// The NAL unit payloads in an Annex-B buffer (start codes `00 00 01` or
/// `00 00 00 01`), with the start codes stripped.
#[must_use]
pub fn annex_b_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut nals = Vec::new();
    let mut start: Option<usize> = None;
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            if let Some(s) = start {
                // Trim the extra leading zero of a 4-byte start code.
                let end = if i > s && data[i - 1] == 0 { i - 1 } else { i };
                if end > s {
                    nals.push(&data[s..end]);
                }
            }
            start = Some(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        nals.push(&data[s..]);
    }
    nals
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annexb(nals: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for nal in nals {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(nal);
        }
        out
    }

    #[test]
    fn split_sorts_params_and_builds_avcc_keyframe() {
        let sps: &[u8] = &[0x67, 0x42, 0x00];
        let pps: &[u8] = &[0x68, 0xce];
        let idr: &[u8] = &[0x65, 0x88, 0x84];
        let s = split_annex_b(&annexb(&[sps, pps, idr]));
        assert!(s.keyframe);
        assert_eq!(s.parameter_sets, vec![sps.to_vec(), pps.to_vec()]);
        let mut expect = Vec::new();
        expect.extend_from_slice(&(idr.len() as u32).to_be_bytes());
        expect.extend_from_slice(idr);
        assert_eq!(s.frame_data, expect);
    }

    #[test]
    fn split_non_keyframe_has_no_params_or_idr() {
        let pslice: &[u8] = &[0x41, 0x9a, 0x00]; // type 1
        let s = split_annex_b(&annexb(&[pslice]));
        assert!(!s.keyframe);
        assert!(s.parameter_sets.is_empty());
        assert_eq!(&s.frame_data[4..], pslice);
    }

    #[test]
    fn split_accepts_three_byte_start_codes() {
        // openh264 emits 4-byte start codes before parameter sets and 3-byte
        // ones before slices; both have to parse or every P-frame is lost.
        let mut buf = vec![0, 0, 0, 1, 0x67, 0x42];
        buf.extend_from_slice(&[0, 0, 1, 0x41, 0x9a]);
        let s = split_annex_b(&buf);
        assert_eq!(s.parameter_sets, vec![vec![0x67, 0x42]]);
        assert_eq!(&s.frame_data[4..], &[0x41, 0x9a]);
    }

    #[test]
    fn pack_rows_crops_to_even_width() {
        // 3x2 source (BGRA), crop to width 2, height 2.
        let src: Vec<u8> = (0..3 * 2 * 4).map(|b| b as u8).collect();
        let mut out = Vec::new();
        pack_rows(&src, 3, 2, 2, &mut out);
        assert_eq!(out.len(), 2 * 2 * 4);
        assert_eq!(&out[0..8], &src[0..8]); // first two pixels of row 0
        assert_eq!(&out[8..16], &src[12..20]); // first two pixels of row 1 (skips px 3)
    }

    #[test]
    fn pack_rows_stops_short_rather_than_panicking() {
        // A capture that handed back fewer bytes than its geometry claims must
        // not take the stream thread down with it.
        let src: Vec<u8> = vec![0; 4 * 4];
        let mut out = Vec::new();
        pack_rows(&src, 2, 2, 8, &mut out);
        assert_eq!(out.len(), 2 * 2 * 4);
    }

    #[test]
    fn encode_dims_caps_the_long_side_and_stays_even() {
        assert_eq!(encode_dims(1920, 1080, 1280), (1280, 720));
        assert_eq!(encode_dims(1080, 1920, 1280), (720, 1280));
        assert_eq!(encode_dims(1024, 768, 1280), (1024, 768)); // under the cap: untouched
        let (w, h) = encode_dims(3000, 17, 1280);
        assert_eq!(w % 2, 0);
        assert_eq!(h % 2, 0);
        assert!(h >= 2); // a very wide desktop must not scale a dimension to zero
    }

    #[test]
    fn fit_passes_through_when_no_scaling_is_needed() {
        let src: Vec<u8> = (0..2 * 2 * 4).map(|b| b as u8).collect();
        let mut scratch = Scratch::new();
        assert_eq!(scratch.fit(&src, 2, (2, 2), (2, 2)), &src[..]);
        assert!(scratch.scaled.is_empty()); // no resize allocation on the common path
    }

    /// The host splits an access unit; the client puts it back together. This
    /// asserts they are actually inverses, using the *client's* own helpers
    /// (`extender_protocol`) rather than a second copy written to agree.
    ///
    /// ⚠️ The order matters and is not arbitrary: parameter sets first, then the
    /// frame's NALs in the order the encoder emitted them. A decoder handed an
    /// IDR before its SPS produces nothing and reports no error.
    #[test]
    fn a_split_access_unit_rebuilds_byte_for_byte_on_the_client_side() {
        let sps: &[u8] = &[0x67, 0x42, 0x00, 0x1e];
        let pps: &[u8] = &[0x68, 0xce, 0x3c, 0x80];
        let idr: &[u8] = &[0x65, 0x88, 0x84, 0x00, 0x21];
        let original = annexb(&[sps, pps, idr]);

        let split = split_annex_b(&original);
        // Exactly what the client does on a keyframe: parameter sets as Annex-B,
        // then the AVCC frame data appended as Annex-B.
        let mut rebuilt = extender_protocol::annex_b_parameter_sets(&split.parameter_sets);
        extender_protocol::append_annex_b(&mut rebuilt, &split.frame_data);

        assert_eq!(rebuilt, original);
    }

    #[test]
    fn fit_scales_to_the_encode_size_and_reuses_its_buffers() {
        let src: Vec<u8> = vec![7; 8 * 8 * 4];
        let mut scratch = Scratch::new();
        assert_eq!(scratch.fit(&src, 8, (8, 8), (4, 4)).len(), 4 * 4 * 4);
        // Second frame through the same Scratch: the result must be the new
        // frame's size, not the old buffer's contents grown or left behind.
        assert_eq!(scratch.fit(&src, 8, (8, 8), (2, 2)).len(), 2 * 2 * 4);
    }
}
