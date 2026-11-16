//! Strict plain-text article wire format. Addresses are never embedded images.
use serde_json::Value;
use std::io::Write;

pub(super) const URL_BYTES: usize = 4096;
const DOCUMENT_BYTES: usize = 1024 * 1024;
const TEXT_BYTES: usize = 512 * 1024;
const BLOCKS: usize = 256;
const IMAGES: usize = 8;

struct Budget(usize);
impl Write for Budget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > DOCUMENT_BYTES.saturating_sub(self.0) {
            return Err(std::io::Error::other("The article exceeds 1 MiB."));
        }
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn dimensions(value: &Value, width: &str, height: &str) -> Result<(), String> {
    for name in [width, height] {
        if value
            .get(name)
            .filter(|size| !size.is_null())
            .is_some_and(|size| {
                !size
                    .as_u64()
                    .is_some_and(|number| (1..=65535).contains(&number))
            })
        {
            return Err("The article image dimensions are invalid.".into());
        }
    }
    Ok(())
}

fn fields(value: &Value, allowed: &[&str]) -> Result<(), String> {
    if !value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| allowed.contains(&key.as_str())))
    {
        return Err("The article contains unsupported fields.".into());
    }
    Ok(())
}

#[derive(Default)]
struct Content {
    bytes: usize,
    list_items: usize,
    images: Vec<String>,
    image_count: usize,
}
impl Content {
    fn text(&mut self, value: &Value, limit: usize) -> Result<(), String> {
        let text = value.as_str().ok_or("The article contains invalid text.")?;
        if text.len() > limit
            || text
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err("The article contains invalid text.".into());
        }
        self.bytes += text.len();
        if self.bytes > TEXT_BYTES {
            return Err("The article text exceeds 512 KiB.".into());
        }
        Ok(())
    }
    fn optional_text(&mut self, value: &Value, field: &str, limit: usize) -> Result<(), String> {
        if let Some(text) = value.get(field).filter(|text| !text.is_null()) {
            self.text(text, limit)?;
        }
        Ok(())
    }
    fn image(&mut self, value: &Value) -> Result<(), String> {
        let address = value
            .as_str()
            .filter(|url| {
                !url.is_empty()
                    && url.len() <= URL_BYTES
                    && !url
                        .chars()
                        .any(|character| character.is_control() || character.is_whitespace())
            })
            .ok_or("The article image address is invalid.")?;
        let url = url::Url::parse(address).map_err(|_| "The article image address is invalid.")?;
        if !matches!(url.scheme(), "https" | "http")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err("The article image address is invalid.".into());
        }
        self.image_count += 1;
        if self.image_count > IMAGES {
            return Err("The article exceeds eight images.".into());
        }
        if !self.images.iter().any(|image| image == address) {
            self.images.push(address.into());
        }
        Ok(())
    }
    fn block(&mut self, block: &Value) -> Result<(), String> {
        match block["type"].as_str() {
            Some("paragraph") => {
                fields(block, &["type", "text"])?;
                self.text(&block["text"], TEXT_BYTES)?;
            }
            Some("heading") => {
                fields(block, &["type", "text", "level"])?;
                if !block["level"]
                    .as_u64()
                    .is_some_and(|level| (1..=6).contains(&level))
                {
                    return Err("The article heading is invalid.".into());
                }
                self.text(&block["text"], TEXT_BYTES)?;
            }
            Some("quote") => {
                fields(block, &["type", "text", "attribution"])?;
                self.text(&block["text"], TEXT_BYTES)?;
                self.optional_text(block, "attribution", 4096)?;
            }
            Some("image") => {
                fields(block, &["type", "url", "alt", "caption", "width", "height"])?;
                dimensions(block, "width", "height")?;
                self.image(&block["url"])?;
                self.optional_text(block, "alt", 4096)?;
                self.optional_text(block, "caption", TEXT_BYTES)?;
            }
            Some("list") => {
                fields(block, &["type", "items", "ordered"])?;
                if !block["ordered"].is_boolean() {
                    return Err("The article list is invalid.".into());
                }
                let items = block["items"]
                    .as_array()
                    .ok_or("The article list is invalid.")?;
                self.list_items += items.len();
                if self.list_items > BLOCKS {
                    return Err("The article exceeds 256 list items.".into());
                }
                for item in items {
                    self.text(item, TEXT_BYTES)?;
                }
            }
            _ => return Err("The article block is unsupported.".into()),
        }
        Ok(())
    }
}

pub(super) fn validate(value: &Value) -> Result<Vec<String>, String> {
    // Count encoded bytes without allocating another serialized copy.
    serde_json::to_writer(Budget(0), value)
        .map_err(|_| "The article exceeds 1 MiB.".to_string())?;
    fields(
        value,
        &["layout", "title", "article", "offline", "stale", "error"],
    )?;
    for flag in ["offline", "stale"] {
        if value.get(flag).is_some_and(|flag| !flag.is_boolean()) {
            return Err("The article cache status is invalid.".into());
        }
    }
    if value["layout"] != "article" {
        return Err("The source did not return an article.".into());
    }
    let article = &value["article"];
    fields(
        article,
        &[
            "id",
            "authors",
            "published_at",
            "summary",
            "image",
            "image_width",
            "image_height",
            "blocks",
        ],
    )?;
    let mut content = Content::default();
    content.optional_text(value, "error", 2048)?;
    content.text(&value["title"], 4096)?;
    content.text(&article["id"], 4096)?;
    if article["id"].as_str().is_none_or(str::is_empty) {
        return Err("The article has no identity.".into());
    }
    content.optional_text(article, "published_at", 128)?;
    content.text(&article["summary"], TEXT_BYTES)?;
    let authors = article["authors"]
        .as_array()
        .filter(|authors| authors.len() <= 32)
        .ok_or("The article authors are invalid.")?;
    for author in authors {
        content.text(author, 512)?;
    }
    dimensions(article, "image_width", "image_height")?;
    if let Some(image) = article.get("image").filter(|image| !image.is_null()) {
        content.image(image)?;
    }
    let blocks = article["blocks"]
        .as_array()
        .filter(|blocks| blocks.len() <= BLOCKS)
        .ok_or("The article exceeds 256 blocks.")?;
    for block in blocks {
        content.block(block)?;
    }
    Ok(content.images)
}
