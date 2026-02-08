#![forbid(unsafe_code)]

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// Web/WASM terminal surface.
///
/// This is the minimal JS-facing API surface. Implementation will evolve to:
/// - own a WebGPU renderer (glyph atlas + instancing),
/// - own web input capture + IME/clipboard,
/// - accept either VT/ANSI byte streams (`feed`) or direct cell diffs
///   (`applyPatch`) for ftui-web mode.
#[wasm_bindgen]
pub struct FrankenTermWeb {
    cols: u16,
    rows: u16,
    initialized: bool,
    canvas: Option<HtmlCanvasElement>,
}

#[wasm_bindgen]
impl FrankenTermWeb {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            cols: 0,
            rows: 0,
            initialized: false,
            canvas: None,
        }
    }

    /// Initialize the terminal surface with an existing `<canvas>`.
    ///
    /// Exported as an async JS function returning a Promise, matching the
    /// `frankenterm-web` architecture spec.
    pub async fn init(
        &mut self,
        canvas: HtmlCanvasElement,
        _options: Option<JsValue>,
    ) -> Result<(), JsValue> {
        self.canvas = Some(canvas);
        self.initialized = true;
        Ok(())
    }

    /// Resize the terminal in logical grid coordinates (cols/rows).
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
    }

    /// Accepts DOM-derived keyboard/mouse/touch events.
    ///
    /// The concrete event schema will be defined once the input layer is built.
    pub fn input(&mut self, _event: JsValue) {}

    /// Feed a VT/ANSI byte stream (remote mode).
    pub fn feed(&mut self, _data: &[u8]) {}

    /// Apply a cell patch (ftui-web mode).
    #[wasm_bindgen(js_name = applyPatch)]
    pub fn apply_patch(&mut self, _patch: JsValue) {}

    /// Request a frame render. In the full implementation this will schedule a
    /// WebGPU pass and present atomically.
    pub fn render(&mut self) {
        if !self.initialized {
            return;
        }
    }

    /// Explicit teardown for JS callers. Clears internal references so the
    /// canvas and any GPU resources can be reclaimed.
    pub fn destroy(&mut self) {
        self.initialized = false;
        self.canvas = None;
    }
}
