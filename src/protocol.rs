use eframe::egui::Color32;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClearLayer {
    #[serde(rename = "2d")]
    TwoD,
    #[serde(rename = "3d")]
    ThreeD,
    All,
}

impl Default for ClearLayer {
    fn default() -> Self {
        Self::All
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DrawStyle {
    Sketch,
    Solid,
}

impl Default for DrawStyle {
    fn default() -> Self {
        Self::Sketch
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormalizedBounds {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Primitive3DType {
    Box,
    Sphere,
    Cylinder,
    Torus,
    Cone,
    Plane,
    Line,
    Billboard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum VisualOp {
    Clear {
        #[serde(default)]
        layer: ClearLayer,
    },
    Backdrop {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        #[serde(default = "default_backdrop_color")]
        color: String,
        #[serde(default = "default_backdrop_radius")]
        radius: f32,
    },
    Pointer {
        x: f32,
        y: f32,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        color: Option<String>,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        #[serde(default = "default_accent_color")]
        color: String,
        #[serde(default)]
        style: DrawStyle,
        #[serde(default)]
        label: Option<String>,
    },
    Circle {
        x: f32,
        y: f32,
        r: f32,
        #[serde(default = "default_accent_color")]
        color: String,
        #[serde(default)]
        style: DrawStyle,
        #[serde(default)]
        label: Option<String>,
    },
    Arrow {
        from: [f32; 2],
        to: [f32; 2],
        #[serde(default = "default_accent_color")]
        color: String,
        #[serde(default)]
        label: Option<String>,
    },
    Stroke {
        points: Vec<[f32; 2]>,
        #[serde(default = "default_ink_color")]
        color: String,
        #[serde(default = "default_stroke_width")]
        width: f32,
    },
    Label {
        x: f32,
        y: f32,
        text: String,
        #[serde(default = "default_ink_color")]
        color: String,
        #[serde(default = "default_label_size")]
        size: f32,
    },
    #[serde(rename = "3d_mount")]
    Mount3D {
        bounds: NormalizedBounds,
        #[serde(default)]
        title: Option<String>,
    },
    #[serde(rename = "3d_unmount")]
    Unmount3D,
    #[serde(rename = "3d_add")]
    Add3D {
        id: String,
        #[serde(rename = "type")]
        kind: Primitive3DType,
        #[serde(default)]
        pos: [f32; 3],
        #[serde(default)]
        rot: [f32; 3],
        #[serde(default = "default_size_3d")]
        size: [f32; 3],
        #[serde(default = "default_3d_color")]
        color: String,
        #[serde(default)]
        spin: Option<[f32; 3]>,
        /// [axis_y_amplitude, speed_rad_per_sec]
        #[serde(default)]
        bob: Option<[f32; 2]>,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        points: Option<Vec<[f32; 3]>>,
        #[serde(default)]
        wireframe: bool,
    },
    #[serde(rename = "3d_update")]
    Update3D {
        id: String,
        #[serde(default)]
        pos: Option<[f32; 3]>,
        #[serde(default)]
        rot: Option<[f32; 3]>,
        #[serde(default)]
        size: Option<[f32; 3]>,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        spin: Option<[f32; 3]>,
        #[serde(default)]
        text: Option<String>,
    },
    #[serde(rename = "3d_remove")]
    Remove3D {
        id: String,
    },
}

fn default_accent_color() -> String {
    "#FF5500".to_string()
}

fn default_ink_color() -> String {
    "#1A1A2E".to_string()
}

fn default_3d_color() -> String {
    "#3B82F6".to_string()
}

fn default_stroke_width() -> f32 {
    3.0
}

fn default_label_size() -> f32 {
    16.0
}

fn default_backdrop_color() -> String {
    "#FAF9F6E6".to_string()
}

fn default_backdrop_radius() -> f32 {
    16.0
}

fn default_size_3d() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

/// Parse a `#RGB`, `#RRGGBB`, or `#RRGGBBAA` hex color string into `Color32`.
pub fn parse_hex_color(hex: &str) -> Color32 {
    let s = hex.trim().trim_start_matches('#');
    match s.len() {
        3 => {
            let r = u8::from_str_radix(&s[0..1], 16).unwrap_or(0) * 17;
            let g = u8::from_str_radix(&s[1..2], 16).unwrap_or(0) * 17;
            let b = u8::from_str_radix(&s[2..3], 16).unwrap_or(0) * 17;
            Color32::from_rgb(r, g, b)
        }
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0);
            Color32::from_rgb(r, g, b)
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0);
            let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0);
            let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0);
            let a = u8::from_str_radix(&s[6..8], 16).unwrap_or(255);
            Color32::from_rgba_unmultiplied(r, g, b, a)
        }
        _ => Color32::from_rgb(255, 85, 0),
    }
}

/// Incremental NDJSON stream parser.
/// Buffers incoming text chunks, splits on `\n`, and parses each complete JSON line.
/// Malformed lines (or markdown code fences) are silently dropped without breaking the stream.
#[derive(Default)]
pub struct NdjsonParser {
    buffer: String,
}

impl NdjsonParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Push a raw text chunk from a token stream and return any newly completed `VisualOp`s.
    pub fn push_chunk(&mut self, chunk: &str) -> Vec<(String, VisualOp)> {
        self.buffer.push_str(chunk);
        let mut ops = Vec::new();
        while let Some(newline_idx) = self.buffer.find('\n') {
            let line = self.buffer[..newline_idx].trim().to_string();
            self.buffer.drain(..=newline_idx);
            if let Some(op) = Self::parse_line(&line) {
                ops.push((line, op));
            }
        }
        ops
    }

    /// Flush any remaining buffered text when a stream finishes.
    pub fn finish(&mut self) -> Vec<(String, VisualOp)> {
        let line = self.buffer.trim().to_string();
        self.buffer.clear();
        if let Some(op) = Self::parse_line(&line) {
            vec![(line, op)]
        } else {
            Vec::new()
        }
    }

    fn parse_line(line: &str) -> Option<VisualOp> {
        if line.is_empty() || line.starts_with("```") {
            return None;
        }
        // If a model wraps JSON with leading prose on the same line, try slicing from first '{'
        let json_slice = if line.starts_with('{') {
            line
        } else if let Some(idx) = line.find('{') {
            &line[idx..]
        } else {
            return None;
        };
        serde_json::from_str::<VisualOp>(json_slice).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_incremental_ndjson_and_ignores_malformed() {
        let mut parser = NdjsonParser::new();
        let ops1 = parser.push_chunk("{\"op\":\"clear\",\"layer\":\"2d\"}\n{\"op\":\"point");
        assert_eq!(ops1.len(), 1);
        assert!(matches!(ops1[0].1, VisualOp::Clear { layer: ClearLayer::TwoD }));

        let ops2 = parser.push_chunk("er\",\"x\":0.42,\"y\":0.61,\"label\":\"Click Export\"}\n{broken json}\n```json\n");
        assert_eq!(ops2.len(), 1);
        assert!(matches!(ops2[0].1, VisualOp::Pointer { .. }));

        let ops3 = parser.push_chunk("{\"op\":\"3d_add\",\"id\":\"cube1\",\"type\":\"box\",\"pos\":[0,0,0],\"size\":[1,1,1],\"color\":\"#3b82f6\",\"spin\":[0,0.01,0]}");
        assert!(ops3.is_empty());
        let ops4 = parser.finish();
        assert_eq!(ops4.len(), 1);
        assert!(matches!(ops4[0].1, VisualOp::Add3D { .. }));
    }

    #[test]
    fn parses_hex_colors() {
        assert_eq!(parse_hex_color("#FF5500"), Color32::from_rgb(255, 85, 0));
        assert_eq!(parse_hex_color("#111"), Color32::from_rgb(17, 17, 17));
        assert_eq!(parse_hex_color("#3b82f680"), Color32::from_rgba_unmultiplied(59, 130, 246, 128));
    }
}
