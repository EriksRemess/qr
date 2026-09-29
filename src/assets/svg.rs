//! Small, dependency-free scanner for trusted SVG assets. This is not an SVG
//! sanitizer or a general XML parser. DTDs/custom entities and external assets are not
//! supported. Comments, processing instructions and CDATA are tokenized so
//! markup-looking text inside them cannot be mistaken for the document root.

use super::{RenderError, attributes};
use crate::render::Color;
use std::fmt::Write;

#[derive(Clone, Debug)]
pub struct SvgLogo {
    pub image_uri: String,
    pub outline_uri: Option<String>,
    pub outline_padding: f64,
    pub outline_width: f64,
    pub outline_marker: String,
    pub inline_svg: Option<String>,
    pub inline_outline: Option<String>,
    pub width: f64,
    pub height: f64,
    document: String,
    view_x: f64,
    view_y: f64,
}

struct Tag<'a> {
    start: usize,
    end: usize,
    name: &'a str,
    attributes: &'a str,
    closing: bool,
    empty: bool,
}

/// Scan tags while checking nesting and requiring exactly one SVG root.
fn tags(input: &str) -> Result<Vec<Tag<'_>>, RenderError> {
    let mut result = Vec::new();
    let mut stack = Vec::new();
    let mut offset = 0;
    let mut saw_root = false;
    while offset < input.len() {
        let remaining = &input[offset..];
        if !remaining.starts_with('<') {
            let length = remaining.find('<').unwrap_or(remaining.len());
            if stack.is_empty() && !remaining[..length].trim().is_empty() {
                return Err(RenderError::InvalidSvgLogo);
            }
            offset += length;
            continue;
        }
        let special = if remaining.starts_with("<!--") {
            Some((4, "-->"))
        } else if remaining.starts_with("<?") {
            Some((2, "?>"))
        } else if remaining.starts_with("<![CDATA[") && !stack.is_empty() {
            Some((9, "]]>"))
        } else {
            None
        };
        if let Some((prefix, terminator)) = special {
            offset += prefix
                + remaining[prefix..]
                    .find(terminator)
                    .ok_or(RenderError::InvalidSvgLogo)?
                + terminator.len();
            continue;
        }
        if remaining.starts_with("<!") {
            return Err(RenderError::InvalidSvgLogo);
        }
        let mut quote = None;
        let end = remaining
            .char_indices()
            .skip(1)
            .find_map(|(index, ch)| {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    if matches!(ch, '\'' | '"') {
                        quote = Some(ch);
                    } else if ch == '>' {
                        return Some(index);
                    }
                }
                None
            })
            .ok_or(RenderError::InvalidSvgLogo)?;
        let mut body = &remaining[1..end];
        let closing = body.starts_with('/');
        if closing {
            body = &body[1..];
        }
        let empty = !closing && body.ends_with('/');
        if empty {
            body = &body[..body.len() - 1];
        }
        let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
        let name = &body[..name_end];
        if name.is_empty()
            || !name
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || b"_:-.".contains(&ch))
        {
            return Err(RenderError::InvalidSvgLogo);
        }
        let attrs = &body[name_end..];
        if closing {
            if !attrs.trim().is_empty() || stack.pop() != Some(name) {
                return Err(RenderError::InvalidSvgLogo);
            }
        } else {
            attributes(attrs)?;
            if stack.is_empty() {
                if saw_root || name != "svg" {
                    return Err(RenderError::InvalidSvgLogo);
                }
                saw_root = true;
            }
            if !empty {
                stack.push(name);
            }
        }
        result.push(Tag {
            start: offset,
            end: offset + end + 1,
            name,
            attributes: attrs,
            closing,
            empty,
        });
        offset += end + 1;
    }
    if !saw_root || !stack.is_empty() {
        return Err(RenderError::InvalidSvgLogo);
    }
    Ok(result)
}

impl SvgLogo {
    pub fn parse(input: &str) -> Result<Self, RenderError> {
        let input = input.trim_start_matches('\u{feff}');
        let parsed = tags(input)?;
        let mut decoded_values = Vec::new();
        for tag in &parsed {
            if !tag.closing {
                for (name, value, _) in attributes(tag.attributes)? {
                    let value = decode_attribute(value)?;
                    if name == "style" {
                        validate_inline_style(&value)?;
                    }
                    decoded_values.push(value);
                }
            }
        }
        // Keep ordinary path-based logos inline for crisp arbitrary zoom even
        // in SVG rasterizers that cache image subdocuments at nominal size.
        // Stylesheets require a separate document to isolate their selectors.
        let has_stylesheet = parsed.iter().any(|tag| {
            matches!(
                tag.name.rsplit(':').next(),
                Some("style" | "script" | "foreignObject")
            )
        });
        let root = &parsed[0];
        let attrs = attributes(root.attributes)?;
        let view_box = attrs
            .iter()
            .find(|(name, _, _)| *name == "viewBox")
            .map(|(_, value, _)| *value)
            .ok_or(RenderError::InvalidSvgLogo)?;
        let view_box = decode_attribute(view_box)?;
        let values = view_box
            .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
            .filter(|value| !value.is_empty())
            .map(str::parse::<f64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| RenderError::InvalidSvgLogo)?;
        if values.len() != 4
            || values.iter().any(|value| !value.is_finite())
            || values[0].abs() > 1e12
            || values[1].abs() > 1e12
            || !(1e-9..=1e12).contains(&values[2])
            || !(1e-9..=1e12).contains(&values[3])
        {
            return Err(RenderError::InvalidSvgLogo);
        }
        let mut document = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\"",
            values[2], values[3]
        );
        let mut root_style = String::new();
        for (name, value, source) in attrs {
            if name == "xmlns" && decode_attribute(value)? != "http://www.w3.org/2000/svg" {
                return Err(RenderError::InvalidSvgLogo);
            }
            if name == "style" {
                root_style = decode_attribute(value)?;
            } else if !matches!(name, "xmlns" | "width" | "height" | "x" | "y") {
                document.push(' ');
                document.push_str(source);
            }
        }
        // Presentation attributes lose to source CSS, including !important.
        // Append our own important geometry declarations last, while retaining
        // unrelated presentation (fill, opacity, fonts, etc.).
        write!(
            document,
            " style=\"{}\"",
            escape_attribute(&viewport_style(&root_style, values[2], values[3], 0.0, 0.0))
        )
        .unwrap();
        document.push('>');
        if !root.empty {
            let close = parsed.last().ok_or(RenderError::InvalidSvgLogo)?;
            document.push_str(&input[root.end..close.start]);
        }
        document.push_str("</svg>");
        // A render-time substitution slot that cannot collide with source text.
        let mut outline_marker = "QR_OUTLINE_WIDTH".to_owned();
        while document.contains(&outline_marker)
            || decoded_values
                .iter()
                .any(|value| value.contains(&outline_marker))
        {
            outline_marker.push('_');
        }
        Ok(Self {
            image_uri: data_uri(&document),
            inline_svg: if has_stylesheet {
                None
            } else {
                Some(namespace_ids(
                    &document,
                    &document_prefix(&document, "main"),
                )?)
            },
            inline_outline: None,
            outline_uri: None,
            outline_padding: 0.0,
            outline_width: 0.0,
            outline_marker,
            width: values[2],
            height: values[3],
            document,
            view_x: values[0],
            view_y: values[1],
        })
    }

    /// Build a separately isolated, stroke-only SVG once at construction. A
    /// group's inherited stroke cannot override a child's own fill/stroke or
    /// CSS. Inline !important declarations on drawable nodes can. Definition
    /// subtrees used for clipping/masking/paint must keep their original fills.
    pub fn set_outline(&mut self, color: Color, width: f64) -> Result<(), RenderError> {
        if color.alpha == 0 || width == 0.0 {
            return Ok(());
        }
        let mut output = String::with_capacity(self.document.len() * 2);
        let mut previous = 0;
        let mut paint_definition_depth = 0_usize;
        for tag in tags(&self.document)? {
            let local_name = tag.name.rsplit(':').next().unwrap();
            let protected = matches!(
                local_name,
                "clipPath" | "mask" | "pattern" | "marker" | "filter"
            );
            if tag.closing && protected {
                paint_definition_depth -= 1;
            }
            let drawable = matches!(
                local_name,
                "path"
                    | "rect"
                    | "circle"
                    | "ellipse"
                    | "polygon"
                    | "polyline"
                    | "line"
                    | "text"
                    | "tspan"
                    | "use"
            );
            let bitmap = matches!(local_name, "image" | "foreignObject");
            if !tag.closing && (drawable || bitmap) && paint_definition_depth == 0 {
                output.push_str(&self.document[previous..tag.start]);
                write!(output, "<{}", tag.name).unwrap();
                let mut style = String::new();
                for (name, value, source) in attributes(tag.attributes)? {
                    if name == "style" {
                        style = decode_attribute(value)?;
                    } else {
                        output.push(' ');
                        output.push_str(source);
                    }
                }
                write!(output, " style=\"{};{}fill:none!important;stroke:#{:02x}{:02x}{:02x}!important;stroke-opacity:{}!important;stroke-width:{}!important;vector-effect:non-scaling-stroke!important;stroke-linejoin:round!important;stroke-linecap:round!important\"{}>",
                    escape_attribute(&style), if bitmap { "display:none!important;" } else { "" }, color.red, color.green, color.blue,
                    f64::from(color.alpha) / 255.0, self.outline_marker, if tag.empty { "/" } else { "" }).unwrap();
                previous = tag.end;
            }
            if !tag.closing && !tag.empty && protected {
                paint_definition_depth += 1;
            }
        }
        output.push_str(&self.document[previous..]);
        // Give strokes room outside the original viewBox. Keep an inner SVG
        // with the original viewport so percentage-based source geometry does
        // not change when the outline viewport grows.
        let padding = width / 2.0;
        let parsed = tags(&output)?;
        let root = &parsed[0];
        let mut inner = format!("<svg x=\"{}\" y=\"{}\"", self.view_x, self.view_y);
        let mut root_style = String::new();
        for (name, value, source) in attributes(root.attributes)? {
            if name == "style" {
                root_style = decode_attribute(value)?;
            } else if !matches!(name, "x" | "y" | "overflow") {
                inner.push(' ');
                inner.push_str(source);
            }
        }
        write!(
            inner,
            " style=\"{};overflow:visible!important\">{}",
            escape_attribute(&viewport_style(
                &root_style,
                self.width,
                self.height,
                self.view_x,
                self.view_y
            )),
            &output[root.end..]
        )
        .unwrap();
        let expanded_width = self.width + padding * 2.0;
        let expanded_height = self.height + padding * 2.0;
        let expanded = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{expanded_width}\" height=\"{expanded_height}\" viewBox=\"{} {} {expanded_width} {expanded_height}\" style=\"{}\">{output}</svg>",
            self.view_x - padding,
            self.view_y - padding,
            viewport_style("", expanded_width, expanded_height, 0.0, 0.0),
            output = inner
        );
        if self.inline_svg.is_some() {
            // Definitions containing non-scaling strokes also depend on the
            // rendered pixel width. Substitute that slot in IDs and references
            // alongside the stroke declarations at render time.
            let prefix = format!(
                "{}{}-",
                document_prefix(&expanded, "outline"),
                self.outline_marker
            );
            self.inline_outline = Some(namespace_ids(&expanded, &prefix)?);
        }
        self.outline_uri = Some(data_uri(&expanded));
        self.outline_padding = padding;
        self.outline_width = width;
        Ok(())
    }
}

/// Stable, non-security FNV-1a-128 content fingerprint. Identical compiled
/// documents may reuse definitions; different assets/styles need separate IDs,
/// even when their QR outputs are embedded inline in the same parent document.
fn document_prefix(document: &str, kind: &str) -> String {
    let mut hash = 0x6c62272e07bb014262b821756295c58d_u128;
    for byte in document.bytes() {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(0x0000000001000000000000000000013b);
    }
    format!("qr-logo-{kind}-{hash:032x}-")
}

/// Inline normal/outline copies must not share definition IDs. Only URL
/// references are rewritten, not arbitrary CSS color literals or text.
fn namespace_ids(document: &str, prefix: &str) -> Result<String, RenderError> {
    let parsed = tags(document)?;
    let mut ids = Vec::new();
    for tag in &parsed {
        if !tag.closing {
            for (name, value, _) in attributes(tag.attributes)? {
                if name == "id" {
                    ids.push(decode_attribute(value)?);
                }
            }
        }
    }
    if ids.is_empty() {
        return Ok(document.to_owned());
    }
    let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
    let mut output = String::with_capacity(document.len());
    let mut previous = 0;
    for tag in parsed {
        if tag.closing {
            continue;
        }
        output.push_str(&document[previous..tag.start]);
        write!(output, "<{}", tag.name).unwrap();
        for (name, value, _) in attributes(tag.attributes)? {
            let value = decode_attribute(value)?;
            let value = if name == "id" {
                format!("{prefix}{value}")
            } else if matches!(name, "href" | "xlink:href") && value.starts_with('#') {
                format!("#{prefix}{}", &value[1..])
            } else {
                namespace_urls(&value, prefix, &ids)
            };
            write!(output, " {name}=\"{}\"", escape_attribute(&value)).unwrap();
        }
        output.push_str(if tag.empty { "/>" } else { ">" });
        previous = tag.end;
    }
    output.push_str(&document[previous..]);
    Ok(output)
}

fn viewport_style(style: &str, width: f64, height: f64, x: f64, y: f64) -> String {
    format!(
        "{style};width:{width}px!important;height:{height}px!important;min-width:0!important;min-height:0!important;max-width:none!important;max-height:none!important;x:{x}px!important;y:{y}px!important"
    )
}

/// Decode standard XML references exactly once before comparing IDs, parsing
/// geometry or editing CSS. Re-escape edited values when serializing them.
fn decode_attribute(input: &str) -> Result<String, RenderError> {
    let mut output = String::with_capacity(input.len());
    let mut remaining = input;
    while let Some(start) = remaining.find('&') {
        output.push_str(&remaining[..start].replace(['\t', '\n', '\r'], " "));
        remaining = &remaining[start + 1..];
        let end = remaining.find(';').ok_or(RenderError::InvalidSvgLogo)?;
        let entity = &remaining[..end];
        let character = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(value) = entity.strip_prefix("#x") {
                    if !value.is_empty() && value.bytes().all(|ch| ch.is_ascii_hexdigit()) {
                        u32::from_str_radix(value, 16).ok()
                    } else {
                        None
                    }
                } else if let Some(value) = entity.strip_prefix('#') {
                    if !value.is_empty() && value.bytes().all(|ch| ch.is_ascii_digit()) {
                        value.parse::<u32>().ok()
                    } else {
                        None
                    }
                } else {
                    None
                }
                .ok_or(RenderError::InvalidSvgLogo)?;
                if !matches!(code, 9 | 10 | 13 | 0x20..=0xd7ff | 0xe000..=0xfffd | 0x10000..=0x10ffff)
                {
                    return Err(RenderError::InvalidSvgLogo);
                }
                char::from_u32(code).ok_or(RenderError::InvalidSvgLogo)?
            }
        };
        output.push(character);
        remaining = &remaining[end + 1..];
    }
    output.push_str(&remaining.replace(['\t', '\n', '\r'], " "));
    Ok(output)
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\t', "&#9;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
}

/// Consume one CSS character/escape, including hex escapes with their optional
/// whitespace terminator. Escaped newlines in strings are continuations.
fn css_character(input: &str) -> (Option<char>, usize) {
    let first = input.chars().next().unwrap();
    if first != '\\' || input.len() == 1 {
        return (Some(first), first.len_utf8());
    }
    let rest = &input[1..];
    let digits = rest
        .bytes()
        .take(6)
        .take_while(u8::is_ascii_hexdigit)
        .count();
    if digits > 0 {
        let code = u32::from_str_radix(&rest[..digits], 16).unwrap();
        let mut consumed = 1 + digits;
        if let Some(ch) = input[consumed..].chars().next().filter(|ch| css_space(*ch)) {
            consumed += ch.len_utf8();
            if ch == '\r' && input[consumed..].starts_with('\n') {
                consumed += 1;
            }
        }
        return (
            Some(
                char::from_u32(code)
                    .filter(|ch| *ch != '\0')
                    .unwrap_or('\u{fffd}'),
            ),
            consumed,
        );
    }
    let ch = rest.chars().next().unwrap();
    if matches!(ch, '\n' | '\r' | '\u{c}') {
        return (None, if rest.starts_with("\r\n") { 3 } else { 2 });
    }
    (Some(ch), 1 + ch.len_utf8())
}

fn css_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// Validate lexical boundaries before appending important declarations. CSS
/// EOF recovery can accept an unfinished comment/string/block, but appending
/// text changes its meaning: our overrides would become part of that token.
/// This checks completeness, not CSS properties or the declaration grammar.
fn validate_inline_style(value: &str) -> Result<(), RenderError> {
    let mut offset = 0;
    let mut blocks = Vec::new();
    while offset < value.len() {
        let rest = &value[offset..];
        let ch = rest.chars().next().unwrap();
        if let Some(comment) = rest.strip_prefix("/*") {
            offset += 4 + comment.find("*/").ok_or(RenderError::InvalidSvgLogo)?;
        } else if matches!(ch, '\'' | '"') {
            offset += 2 + css_string_end(&rest[1..], ch).ok_or(RenderError::InvalidSvgLogo)?;
        } else if ch.is_alphanumeric() || matches!(ch, '_' | '-' | '\\') || !ch.is_ascii() {
            let start = offset;
            while offset < value.len() {
                let rest = &value[offset..];
                let ch = rest.chars().next().unwrap();
                if ch == '\\' {
                    if rest.len() == 1 {
                        return Err(RenderError::InvalidSvgLogo);
                    }
                    offset += css_character(rest).1;
                } else if ch.is_alphanumeric() || matches!(ch, '_' | '-') || !ch.is_ascii() {
                    offset += ch.len_utf8();
                } else {
                    break;
                }
            }
            // An unquoted URL is one token: comment-looking bytes inside it
            // are literal, unlike comments after a quoted URL argument.
            if decode_css(&value[start..offset]).eq_ignore_ascii_case("url")
                && value[offset..].starts_with('(')
            {
                let (_, consumed) =
                    css_url_argument(&value[offset + 1..]).ok_or(RenderError::InvalidSvgLogo)?;
                offset += 1 + consumed;
            }
        } else {
            match ch {
                '(' => blocks.push(')'),
                '[' => blocks.push(']'),
                '{' => blocks.push('}'),
                ')' | ']' | '}' if blocks.pop() != Some(ch) => {
                    return Err(RenderError::InvalidSvgLogo);
                }
                _ => {}
            }
            offset += ch.len_utf8();
        }
    }
    if !blocks.is_empty() {
        return Err(RenderError::InvalidSvgLogo);
    }
    Ok(())
}

fn decode_css(input: &str) -> String {
    let mut rest = input;
    let mut result = String::new();
    while !rest.is_empty() {
        let (ch, consumed) = css_character(rest);
        if let Some(ch) = ch {
            result.push(ch);
        }
        rest = &rest[consumed..];
    }
    result
}

/// Locate a string's closing quote; comments and the other quote are literal
/// characters inside a CSS string, while escaped quotes do not close it.
fn css_string_end(input: &str, quote: char) -> Option<usize> {
    let mut offset = 0;
    while offset < input.len() {
        let rest = &input[offset..];
        let ch = rest.chars().next().unwrap();
        if ch == '\\' {
            offset += css_character(rest).1;
            continue;
        }
        if ch == quote {
            return Some(offset);
        }
        offset += ch.len_utf8();
    }
    None
}

/// Consume a URL argument and its closing parenthesis. Quoted URL strings can
/// be followed by whitespace/comments before `)`. In an unquoted URL, comment
/// markers are URL characters, not CSS comments; do not strip them globally.
fn css_url_argument(input: &str) -> Option<(&str, usize)> {
    let content = input.trim_start_matches(css_space);
    let leading = input.len() - content.len();
    if let Some(quote @ ('\'' | '"')) = content.chars().next() {
        let end = 1 + css_string_end(&content[1..], quote)?;
        let reference = &content[1..end];
        let mut tail = &content[end + 1..];
        loop {
            tail = tail.trim_start_matches(css_space);
            if let Some(comment) = tail.strip_prefix("/*") {
                tail = &comment[comment.find("*/")? + 2..];
            } else {
                return tail
                    .starts_with(')')
                    .then_some((reference, input.len() - tail.len() + 1));
            }
        }
    }
    let mut offset = 0;
    while offset < content.len() {
        let rest = &content[offset..];
        let ch = rest.chars().next().unwrap();
        if ch == '\\' {
            offset += css_character(rest).1;
        } else if ch == ')' {
            return Some((
                content[..offset].trim_end_matches(css_space),
                leading + offset + 1,
            ));
        } else if matches!(ch, '\'' | '"' | '(') {
            return None;
        } else {
            offset += ch.len_utf8();
        }
    }
    None
}

fn namespace_urls(value: &str, prefix: &str, ids: &[&str]) -> String {
    let mut output = String::with_capacity(value.len());
    let mut offset = 0;
    let mut copied = 0;
    while offset < value.len() {
        let rest = &value[offset..];
        let ch = rest.chars().next().unwrap();
        // Do not rewrite URL-looking text inside comments or string values.
        if rest.starts_with("/*") {
            offset += rest.find("*/").map_or(rest.len(), |end| end + 2);
            continue;
        }
        if matches!(ch, '\'' | '"') {
            offset += 1 + css_string_end(&rest[1..], ch).map_or(rest.len() - 1, |end| end + 1);
            continue;
        }
        if !(ch.is_alphanumeric() || matches!(ch, '_' | '-' | '\\') || !ch.is_ascii()) {
            offset += ch.len_utf8();
            continue;
        }
        let start = offset;
        while offset < value.len() {
            let ch = value[offset..].chars().next().unwrap();
            if ch == '\\' {
                offset += css_character(&value[offset..]).1;
            } else if ch.is_alphanumeric() || matches!(ch, '_' | '-') || !ch.is_ascii() {
                offset += ch.len_utf8();
            } else {
                break;
            }
        }
        if !decode_css(&value[start..offset]).eq_ignore_ascii_case("url")
            || !value[offset..].starts_with('(')
        {
            continue;
        }
        let content_start = offset + 1;
        let Some((reference, consumed)) = css_url_argument(&value[content_start..]) else {
            break;
        };
        offset = content_start + consumed;
        let decoded = decode_css(reference);
        if let Some(id) = decoded.strip_prefix('#').filter(|id| ids.contains(id)) {
            output.push_str(&value[copied..start]);
            // Always serialize a quoted CSS string; escaped punctuation in an
            // ID must not become URL syntax after decoding.
            let escaped = format!("#{prefix}{id}")
                .chars()
                .fold(String::new(), |mut text, ch| {
                    if matches!(ch, '\\' | '"') || ch.is_control() {
                        write!(text, "\\{:x} ", u32::from(ch)).unwrap();
                    } else {
                        text.push(ch);
                    }
                    text
                });
            write!(output, "url(\"{escaped}\")").unwrap();
            copied = offset;
        }
    }
    output.push_str(&value[copied..]);
    output
}

/// Percent encoding is intentionally small and dependency-free. Only URI
/// unreserved bytes survive, so XML, CSS, quotes and Unicode cannot escape the
/// image's href attribute. The embedded resource stays vector, not raster.
fn data_uri(document: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut result = String::from("data:image/svg+xml,");
    for byte in document.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            result.push(char::from(byte));
        } else {
            result.push('%');
            result.push(char::from(HEX[(byte >> 4) as usize]));
            result.push(char::from(HEX[(byte & 15) as usize]));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_entities_decode_once_and_round_trip() {
        let value = decode_attribute("&amp;#35;&#x23;&#35;&quot;&apos;&#10;&#9;&lt;&gt;ā").unwrap();
        assert_eq!(value, "&#35;##\"'\n\t<>ā");
        assert_eq!(decode_attribute(&escape_attribute(&value)).unwrap(), value);
        for invalid in [
            "&#0;",
            "&#xD800;",
            "&#x110000;",
            "&custom;",
            "&#;",
            "&#x;",
            "&#+35;",
            "&#x+23;",
        ] {
            assert!(decode_attribute(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn outline_slot_does_not_collide_with_encoded_source_attributes() {
        let mut logo = SvgLogo::parse("<svg viewBox='0 0 10 10'><rect id='QR_OUTLINE_&#87;IDTH' width='10' height='10'/></svg>").unwrap();
        logo.set_outline(Color::WHITE, 1.0).unwrap();
        assert_eq!(logo.outline_marker, "QR_OUTLINE_WIDTH_");
        assert!(
            logo.inline_outline
                .as_ref()
                .unwrap()
                .contains("-QR_OUTLINE_WIDTH\"")
        );
    }

    #[test]
    fn reference_rewriting_is_single_pass() {
        let value = "fill:URL( '#a' );clip-path:url( \"#qr-logo-main-a\" );stroke:#a";
        assert_eq!(
            namespace_urls(value, "qr-logo-main-", &["a", "qr-logo-main-a"]),
            "fill:url(\"#qr-logo-main-a\");clip-path:url(\"#qr-logo-main-qr-logo-main-a\");stroke:#a"
        );
    }

    #[test]
    fn css_escapes_resolve_without_rewriting_strings_or_comments() {
        for input in [
            r"url(#\70 aint)",
            r"url(#\000070aint)",
            r"u\72l('\23 paint')",
            "url(\"#pa\\\nint\")",
        ] {
            assert_eq!(
                namespace_urls(input, "scope-", &["paint"]),
                "url(\"#scope-paint\")"
            );
        }
        assert_eq!(
            namespace_urls(r"url(#a\)b)", "scope-", &["a)b"]),
            "url(\"#scope-a)b\")"
        );
        assert_eq!(
            namespace_urls(r#"url('#a\"b')"#, "scope-", &["a\"b"]),
            r##"url("#scope-a\22 b")"##
        );
        let untouched =
            "content:'url(#paint)';/* url(#paint) */fill:myurl(#paint);stroke:url(other.svg#paint)";
        assert_eq!(namespace_urls(untouched, "scope-", &["paint"]), untouched);
    }

    #[test]
    fn quoted_urls_allow_comments_after_the_string() {
        for value in [
            "url('#paint'/**/)",
            "url( \"#paint\" /* ) '\" url(#other) */ /**/ )",
            "url('#\\70 aint'/*one*//*two*/)",
        ] {
            assert_eq!(
                namespace_urls(value, "scope-", &["paint"]),
                "url(\"#scope-paint\")"
            );
        }
        // Comment syntax and opposite quotes inside a string are literal.
        assert_eq!(
            namespace_urls("url(\"#a'/*b*/\"/**/)", "scope-", &["a'/*b*/"]),
            "url(\"#scope-a'/*b*/\")"
        );
        let value = "content:'a\" url(#paint)';fill:url('#paint'/**/)";
        assert_eq!(
            namespace_urls(value, "scope-", &["paint"]),
            "content:'a\" url(#paint)';fill:url(\"#scope-paint\")"
        );
        for untouched in [
            "url(#paint/**/)",
            "url('#paint'/*unfinished)",
            "url('#paint' extra)",
        ] {
            assert_eq!(namespace_urls(untouched, "scope-", &["paint"]), untouched);
        }
    }

    #[test]
    fn inline_styles_must_end_at_a_complete_css_boundary() {
        for invalid in [
            "fill:red;/*",
            "fill:red;/*unfinished",
            "font-family:'unfinished",
            "font-family:\"unfinished",
            "fill:red;--x:func(",
            "--x:[value",
            "--x:{value",
            "--x:([)]",
            "--x:value\\",
            "fill:url('#paint'/*)",
            "fill:url(#paint",
            r"fill:u\72l('#paint'/*)",
        ] {
            assert!(validate_inline_style(invalid).is_err(), "{invalid}");
        }
        for valid in [
            "fill:red;/**/",
            "font-family:'/* not a comment'",
            "--x:'a\"b'",
            "--x:func([{}]);fill:red",
            "--x:escaped\\(value",
            "--x:escaped\\\\",
            "fill:url(#paint/*)",
            "fill:url('#paint'/**/)",
            "fill:url('data:image/svg+xml,/*')",
            r"fill:u\72l(#paint/*)",
            "--x:'continued\\\nstring'",
        ] {
            assert!(validate_inline_style(valid).is_ok(), "{valid}");
        }
    }

    #[test]
    fn comment_markup_is_not_a_root_and_empty_roots_are_supported() {
        let logo =
            SvgLogo::parse("<!-- <svg> --><svg viewBox='0 0 10 10'/><!-- </svg> -->").unwrap();
        assert_eq!(logo.width, 10.0);
        assert!(!logo.document.contains("<!--"));
        assert!(SvgLogo::parse("<svg viewBox='0 0 10 10'><g></svg>").is_err());
    }
}
