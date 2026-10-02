//! Bounded, self-contained vector icons. No scripts, foreign objects or network references.
pub(crate) fn validate(bytes: &[u8]) -> Result<(), String> {
    let invalid = || "Use a self-contained SVG with vector shapes and internal references.".to_string();
    if bytes.len() > 256 * 1024 { return Err("The SVG icon is too large.".into()); }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid())?;
    let document = roxmltree::Document::parse(text).map_err(|_| invalid())?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" || document.descendants().count() > 5000 { return Err(invalid()); }
    for node in document.descendants().filter(|node| node.is_element()) {
        if node.ancestors().count() > 64 || node.tag_name().namespace().is_some_and(|ns| ns != "http://www.w3.org/2000/svg") { return Err(invalid()); }
        if !matches!(node.tag_name().name(), "svg" | "g" | "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "defs" | "use" | "symbol" | "clipPath" | "mask" | "linearGradient" | "radialGradient" | "stop" | "title" | "desc") { return Err(invalid()); }
        for attribute in node.attributes() {
            let value = attribute.value().trim();
            let name = attribute.name();
            if !matches!(name, "id" | "version" | "viewBox" | "preserveAspectRatio" | "width" | "height" | "x" | "y" | "x1" | "y1" | "x2" | "y2" | "cx" | "cy" | "r" | "rx" | "ry" | "fx" | "fy" | "d" | "points" | "transform" | "fill" | "fill-rule" | "fill-opacity" | "stroke" | "stroke-width" | "stroke-linecap" | "stroke-linejoin" | "stroke-miterlimit" | "stroke-dasharray" | "stroke-dashoffset" | "stroke-opacity" | "opacity" | "color" | "clip-path" | "clip-rule" | "mask" | "maskUnits" | "maskContentUnits" | "clipPathUnits" | "gradientUnits" | "gradientTransform" | "spreadMethod" | "offset" | "stop-color" | "stop-opacity" | "href" | "class" | "role" | "aria-hidden" | "focusable") { return Err(invalid()); }
            if name == "href" && (!value.starts_with('#') || value.len() < 2) { return Err(invalid()); }
            if matches!(name, "fill" | "stroke" | "clip-path" | "mask") && value.to_ascii_lowercase().contains("url") {
                let reference = value.strip_prefix("url(#").and_then(|value| value.strip_suffix(')')).ok_or_else(invalid)?;
                if reference.is_empty() || !reference.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte)) { return Err(invalid()); }
            }
            if matches!(name, "width" | "height") && value.trim_end_matches("px").parse::<f64>().is_ok_and(|size| !size.is_finite() || size > 4096.0) { return Err(invalid()); }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_paths_and_internal_gradients_are_supported_without_external_resources() {
        assert!(validate(br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor"><path d="M2 2L22 22"/><defs><linearGradient id="tone"><stop offset="0" stop-color="#fff"/></linearGradient></defs><circle r="2" fill="url(#tone)"/></svg>"##).is_ok());
        for source in [r#"<svg><script/></svg>"#, r#"<svg><image href="https://example.org/a.png"/></svg>"#,
            r#"<svg onload="run()"/>"#, r#"<svg><use href="file:///private"/></svg>"#,
            r#"<svg><path fill="url(https://example.org/a)"/></svg>"#,
            r#"<!DOCTYPE svg [<!ENTITY x SYSTEM "file:///private">]><svg>&x;</svg>"#] {
            assert!(validate(source.as_bytes()).is_err());
        }
    }
}
