//! Maps QuickJS `ctx` bridge method names to manifest `permissions` string keys.

/// Returns the required manifest permission key for a `ctx` bridge method, or `None` if unchecked.
pub fn manifest_permission_key_for_ctx_method(method: &str) -> Option<&'static str> {
    match method {
        // Capture / frame access
        "capture" | "captureRegion" | "getScreenSize" => Some("screenshot"),

        // Vision
        "findTemplate" | "findTemplates" | "findTemplateBatch" => Some("template_match"),
        "ocr" | "ocrAll" => Some("ocr"),
        "getColor" | "colorMatch" | "colorMatchAll" | "scanSliderStrip" | "scanStripEdges" | "countColor" => {
            Some("color_detect")
        }

        // Input
        "click" | "doubleClick" | "rightClick" | "mouseMove" | "mouseDown" | "mouseUp"
        | "scroll" | "swipe" | "keyDown" | "keyUp" | "keyPress" | "keyCombo" | "typeText"
        | "replay" => {
            Some("click")
        }

        // Wait (time) — needs template / color / vision as appropriate
        "waitForTemplate" | "waitGone" | "waitForTemplateFrames" | "waitGoneFrames" => {
            Some("template_match")
        }
        "waitForColor" | "waitForColorFrames" => Some("color_detect"),

        "sleep" | "sleepFrames" => None,

        // Window
        "findWindow" | "activateWindow" | "getWindowRect" => Some("window"),

        // Inter-script
        "runScript" => Some("call_script"),
        "call" => Some("call_library"),

        "notify" => Some("notify"),

        "readStoreFile" | "writeStoreFile" | "listStoreFiles" => Some("storage"),

        "readFile" | "writeFile" | "listFiles" | "fileExists" => Some("file"),

        "httpGet" | "httpPost" => Some("network"),

        "storageGet" | "storageSet" | "storageDelete" | "storageKeys" => Some("storage"),

        _ => None,
    }
}
