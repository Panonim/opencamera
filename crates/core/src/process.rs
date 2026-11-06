//! Turns, flips, resizes and adjusts the brightness of each frame to fit the virtual camera picture.

use serde::{Deserialize, Serialize};

/// Picture size the virtual camera outputs, incoming frames are fitted to it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum OutputSize {
    #[serde(rename = "720p")]
    P720,
    #[default]
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "2160p")]
    P2160,
}

impl OutputSize {
    pub fn dimensions(self) -> (usize, usize) {
        match self {
            Self::P720 => (1280, 720),
            Self::P1080 => (1920, 1080),
            Self::P2160 => (3840, 2160),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Framing {
    #[default]
    Fit,
    Fill,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Placeholder {
    #[default]
    Card,
    Black,
    Freeze,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProcessingSettings {
    pub mirror: bool,
    /// Clockwise turn in degrees: 0, 90, 180 or 270.
    pub rotation: u16,
    /// Brightness change from -100 to 100.
    pub brightness: i16,
    /// Contrast change from -100 to 100.
    pub contrast: i16,
    pub framing: Framing,
    pub placeholder: Placeholder,
    pub output_size: OutputSize,
}

/// A decoded video frame borrowed from the decoder.
pub struct I420<'a> {
    pub width: usize,
    pub height: usize,
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub y_stride: usize,
    pub uv_stride: usize,
}

#[derive(PartialEq, Clone, Copy)]
struct MapKey {
    out_w: usize,
    out_h: usize,
    width: usize,
    height: usize,
    y_stride: usize,
    uv_stride: usize,
    mirror: bool,
    rotation: u16,
    framing: Framing,
}

/// Lookup tables of which source pixel feeds each output row and column, with -1 meaning a black bar.
#[derive(Default)]
struct Maps {
    y_col: Vec<isize>,
    y_row: Vec<isize>,
    c_col: Vec<isize>,
    c_row: Vec<isize>,
}

pub struct Processor {
    settings: ProcessingSettings,
    key: Option<MapKey>,
    maps: Maps,
    lut: [u8; 256],
    /// Output width and height, matching the length of `out`.
    size: (usize, usize),
    out: Vec<u8>,
}

impl Processor {
    pub fn new(settings: ProcessingSettings) -> Self {
        let mut p = Self {
            settings,
            key: None,
            maps: Maps::default(),
            lut: [0; 256],
            size: (0, 0),
            out: Vec::new(),
        };
        p.set_settings(settings);
        p
    }

    pub fn settings(&self) -> ProcessingSettings {
        self.settings
    }

    /// Output width and height in pixels.
    pub fn size(&self) -> (usize, usize) {
        self.size
    }

    pub fn set_settings(&mut self, s: ProcessingSettings) {
        self.settings = s;
        self.key = None;
        if self.size != s.output_size.dimensions() {
            self.size = s.output_size.dimensions();
            self.out = vec![0; self.size.0 * self.size.1 * 3 / 2];
            fill_black(&mut self.out, self.size);
        }
        let gain = (100 + s.contrast.clamp(-100, 100)) as f32 / 100.0;
        let offset = s.brightness.clamp(-100, 100) as f32 * 1.28;
        for (i, v) in self.lut.iter_mut().enumerate() {
            *v = ((i as f32 - 128.0) * gain + 128.0 + offset).round().clamp(16.0, 235.0) as u8;
        }
    }

    pub fn output(&self) -> &[u8] {
        &self.out
    }

    pub fn process(&mut self, f: &I420) -> &[u8] {
        let (ow, oh) = self.size;
        let key = MapKey {
            out_w: ow,
            out_h: oh,
            width: f.width,
            height: f.height,
            y_stride: f.y_stride,
            uv_stride: f.uv_stride,
            mirror: self.settings.mirror,
            rotation: self.settings.rotation,
            framing: self.settings.framing,
        };
        if self.key != Some(key) {
            self.maps = build_maps(&key);
            self.key = Some(key);
        }
        let (luma, chroma) = self.out.split_at_mut(ow * oh);
        for (oy, row) in luma.chunks_exact_mut(ow).enumerate() {
            let r = self.maps.y_row[oy];
            if r < 0 {
                row.fill(16);
                continue;
            }
            for (px, &c) in row.iter_mut().zip(&self.maps.y_col) {
                *px = if c < 0 { 16 } else { self.lut[f.y[(r + c) as usize] as usize] };
            }
        }
        for (cy, row) in chroma.chunks_exact_mut(ow).enumerate() {
            let r = self.maps.c_row[cy];
            for (pair, &c) in row.chunks_exact_mut(2).zip(&self.maps.c_col) {
                if r < 0 || c < 0 {
                    pair.copy_from_slice(&[128, 128]);
                } else {
                    let i = (r + c) as usize;
                    pair.copy_from_slice(&[f.u[i], f.v[i]]);
                }
            }
        }
        &self.out
    }

    /// Shows the chosen picture for when no phone is connected, or keeps the last frame.
    pub fn placeholder(&mut self) -> &[u8] {
        match self.settings.placeholder {
            Placeholder::Freeze => {}
            Placeholder::Black => fill_black(&mut self.out, self.size),
            Placeholder::Card => draw_card(&mut self.out, self.size),
        }
        &self.out
    }
}

fn fill_black(out: &mut [u8], (w, h): (usize, usize)) {
    out[..w * h].fill(16);
    out[w * h..].fill(128);
}

fn build_maps(k: &MapKey) -> Maps {
    let (w, h) = (k.width, k.height);
    let (ow, oh) = (k.out_w, k.out_h);
    let swap = k.rotation % 180 != 0;
    let (rw, rh) = if swap { (h, w) } else { (w, h) };
    let sx = ow as f64 / rw as f64;
    let sy = oh as f64 / rh as f64;
    let scale = if k.framing == Framing::Fit { sx.min(sy) } else { sx.max(sy) };
    let x0 = (ow as f64 - rw as f64 * scale) / 2.0;
    let y0 = (oh as f64 - rh as f64 * scale) / 2.0;

    // Finds where each output pixel lands in the turned and flipped picture.
    let u_of = |ox: usize| {
        let u = ((ox as f64 + 0.5 - x0) / scale).floor();
        (u >= 0.0 && (u as usize) < rw).then(|| if k.mirror { rw - 1 - u as usize } else { u as usize })
    };
    let v_of = |oy: usize| {
        let v = ((oy as f64 + 0.5 - y0) / scale).floor();
        (v >= 0.0 && (v as usize) < rh).then_some(v as usize)
    };
    // Converts a position in the turned picture back to a position in the original frame.
    let from_u = |u: usize| match k.rotation {
        90 => h - 1 - u,
        180 => w - 1 - u,
        _ => u,
    };
    let from_v = |v: usize| match k.rotation {
        180 => h - 1 - v,
        270 => w - 1 - v,
        _ => v,
    };
    let axis = |coord: Option<usize>, f: &dyn Fn(usize) -> usize, stride: usize, div: usize| -> isize {
        coord.map_or(-1, |c| ((f(c) / div) * stride) as isize)
    };
    let (u_stride_y, v_stride_y) = if swap { (k.y_stride, 1) } else { (1, k.y_stride) };
    let (u_stride_c, v_stride_c) = if swap { (k.uv_stride, 1) } else { (1, k.uv_stride) };
    Maps {
        y_col: (0..ow).map(|ox| axis(u_of(ox), &from_u, u_stride_y, 1)).collect(),
        y_row: (0..oh).map(|oy| axis(v_of(oy), &from_v, v_stride_y, 1)).collect(),
        c_col: (0..ow / 2).map(|cx| axis(u_of(cx * 2), &from_u, u_stride_c, 2)).collect(),
        c_row: (0..oh / 2).map(|cy| axis(v_of(cy * 2), &from_v, v_stride_c, 2)).collect(),
    }
}

/// Draws a plain gray screen with a simple camera icon, sized as it looks at 1080p.
fn draw_card(out: &mut [u8], (ow, oh): (usize, usize)) {
    let (luma, chroma) = out.split_at_mut(ow * oh);
    chroma.fill(128);
    let (cx, cy, oh) = (ow as i32 / 2, oh as i32 / 2, oh as i32);
    for y in 0..oh {
        for x in 0..ow as i32 {
            let (dx, dy) = ((x - cx) * 1080 / oh, (y - cy) * 1080 / oh);
            let body = dx.abs() <= 150 && dy.abs() <= 100;
            let bump = dx.abs() <= 50 && (-135..=-100).contains(&dy);
            let lens = dx * dx + dy * dy <= 60 * 60;
            let ring = dx * dx + dy * dy <= 75 * 75;
            luma[(y as usize) * ow + x as usize] = if lens {
                40
            } else if ring {
                120
            } else if body || bump {
                90
            } else {
                34
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OW: usize = 1920;
    const OH: usize = 1080;

    fn frame(w: usize, h: usize, f: impl Fn(usize, usize) -> u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let y = (0..h).flat_map(|r| (0..w).map(move |c| (r, c))).map(|(r, c)| f(c, r)).collect();
        (y, vec![100; w * h / 4], vec![150; w * h / 4])
    }

    fn run(settings: ProcessingSettings, w: usize, h: usize, f: impl Fn(usize, usize) -> u8) -> Vec<u8> {
        let (y, u, v) = frame(w, h, f);
        let mut p = Processor::new(settings);
        p.process(&I420 { width: w, height: h, y: &y, u: &u, v: &v, y_stride: w, uv_stride: w / 2 }).to_vec()
    }

    fn luma(out: &[u8], x: usize, y: usize) -> u8 {
        out[y * OW + x]
    }

    #[test]
    fn same_size_is_identity() {
        let out = run(ProcessingSettings::default(), OW, OH, |x, y| (16 + (x + y) % 200) as u8);
        assert_eq!(luma(&out, 0, 0), 16);
        assert_eq!(luma(&out, 10, 3), 29);
        assert_eq!(out[OW * OH], 100);
        assert_eq!(out[OW * OH + 1], 150);
    }

    #[test]
    fn portrait_is_letterboxed_when_fit() {
        let out = run(ProcessingSettings::default(), 720, 1280, |_, _| 200);
        assert_eq!(luma(&out, 10, OH / 2), 16, "left bar is black");
        assert_eq!(luma(&out, OW / 2, OH / 2), 200);
    }

    #[test]
    fn portrait_fills_when_fill() {
        let s = ProcessingSettings { framing: Framing::Fill, ..Default::default() };
        let out = run(s, 720, 1280, |_, _| 200);
        assert_eq!(luma(&out, 0, 0), 200);
    }

    #[test]
    fn mirror_flips_horizontally() {
        let s = ProcessingSettings { mirror: true, ..Default::default() };
        let out = run(s, OW, OH, |x, _| if x < OW / 2 { 50 } else { 200 });
        assert_eq!(luma(&out, 0, 0), 200);
        assert_eq!(luma(&out, OW - 1, 0), 50);
    }

    #[test]
    fn rotation_90_moves_bottom_left_to_top_left() {
        let s = ProcessingSettings { rotation: 90, framing: Framing::Fill, ..Default::default() };
        // A tall frame with a bright bottom edge should show that edge on the left after turning.
        let out = run(s, 1080, 1920, |_, y| if y >= 1900 { 220 } else { 30 });
        assert_eq!(luma(&out, 0, OH / 2), 220);
        assert_eq!(luma(&out, OW - 1, OH / 2), 30);
    }

    #[test]
    fn brightness_and_contrast_lut() {
        let p = Processor::new(ProcessingSettings { brightness: 50, ..Default::default() });
        assert_eq!(p.lut[128], 192);
        let p = Processor::new(ProcessingSettings { contrast: -100, ..Default::default() });
        assert_eq!(p.lut[16], 128);
        assert_eq!(p.lut[235], 128);
    }

    #[test]
    fn output_matches_chosen_size() {
        for (size, w, h) in [(OutputSize::P720, 1280, 720), (OutputSize::P1080, 1920, 1080), (OutputSize::P2160, 3840, 2160)] {
            let s = ProcessingSettings { output_size: size, ..Default::default() };
            let out = run(s, 640, 360, |_, _| 200);
            assert_eq!(out.len(), w * h * 3 / 2);
            assert_eq!(out[(h / 2) * w + w / 2], 200);
            assert_eq!(Processor::new(s).placeholder().len(), w * h * 3 / 2);
        }
    }

    #[test]
    fn changing_size_resizes_output() {
        let mut p = Processor::new(ProcessingSettings { placeholder: Placeholder::Freeze, ..Default::default() });
        assert_eq!(p.size(), (OW, OH));
        p.set_settings(ProcessingSettings { output_size: OutputSize::P720, ..p.settings() });
        assert_eq!(p.size(), (1280, 720));
        // A frozen picture after resizing is a valid black frame of the new size.
        let out = p.placeholder();
        assert_eq!(out.len(), 1280 * 720 * 3 / 2);
        assert_eq!((out[0], out[1280 * 720]), (16, 128));
    }

    #[test]
    fn output_size_serde() {
        let s: ProcessingSettings = serde_json::from_str(r#"{"mirror":true}"#).unwrap();
        assert_eq!(s.output_size, OutputSize::P1080);
        let s: ProcessingSettings = serde_json::from_str(r#"{"outputSize":"2160p"}"#).unwrap();
        assert_eq!(s.output_size, OutputSize::P2160);
        assert!(serde_json::to_string(&s).unwrap().contains(r#""outputSize":"2160p""#));
    }
}
