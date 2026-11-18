use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::normalize::validate_and_normalize_url;

fn parser_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH
}

/// Normalize all line endings (`\r\n` and `\r`) to LF (`\n`) per Spec 8.1.
pub fn normalize_line_endings(input: &str) -> String {
    input.replace("\r\n", "\n").replace('\r', "\n")
}

/// Escape text safely for HTML text nodes and double-quoted attribute values.
pub fn escape_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn push_space_if_needed(out: &mut String) {
    if !out.is_empty() && !out.ends_with(char::is_whitespace) {
        out.push(' ');
    }
}

/// Extract searchable text from Markdown per Spec 6.2:
/// - Visible text concatenated, block boundaries separated by space
/// - Link labels and image alt text included
/// - Raw HTML included as visible text
pub fn extract_search_text(markdown: &str) -> String {
    let normalized = normalize_line_endings(markdown);
    let parser = Parser::new_ext(&normalized, parser_options());
    let mut out = String::new();

    for event in parser {
        match event {
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
                out.push_str(&text);
            }
            Event::SoftBreak | Event::HardBreak | Event::Rule => {
                push_space_if_needed(&mut out);
            }
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::BlockQuote(_)
                | TagEnd::CodeBlock
                | TagEnd::HtmlBlock
                | TagEnd::Item
                | TagEnd::TableCell
                | TagEnd::TableRow
                | TagEnd::TableHead
                | TagEnd::Table => {
                    push_space_if_needed(&mut out);
                }
                _ => {}
            },
            _ => {}
        }
    }

    out.trim().to_string()
}

struct LinkFrame {
    dest_url: String,
    title: String,
    buffer: String,
}

struct ImageFrame {
    alt_text: String,
}

/// Render Markdown safely for read preview per Spec 8.1 and AT-22:
/// - CommonMark + GFM tables & tasklists
/// - Raw HTML rendered as escaped text
/// - Images rendered as `[画像: alt]` text only (no `<img>` tag, no external fetching)
/// - Links validated via `validate_and_normalize_url`; only valid http(s) links become clickable external buttons
pub fn render_markdown_preview(markdown: &str) -> String {
    let normalized = normalize_line_endings(markdown);
    let parser = Parser::new_ext(&normalized, parser_options());

    let mut root_html = String::new();
    let mut link_stack: Vec<LinkFrame> = Vec::new();
    let mut image_stack: Vec<ImageFrame> = Vec::new();
    let mut table_alignments: Vec<Alignment> = Vec::new();
    let mut in_table_head = false;
    let mut col_index: usize = 0;

    fn target_buf<'a>(
        root: &'a mut String,
        links: &'a mut [LinkFrame],
        images: &'a mut [ImageFrame],
    ) -> &'a mut String {
        if let Some(img) = images.last_mut() {
            &mut img.alt_text
        } else if let Some(link) = links.last_mut() {
            &mut link.buffer
        } else {
            root
        }
    }

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Image { .. } => {
                    image_stack.push(ImageFrame {
                        alt_text: String::new(),
                    });
                }
                Tag::Link {
                    dest_url, title, ..
                } => {
                    if image_stack.is_empty() {
                        link_stack.push(LinkFrame {
                            dest_url: dest_url.into_string(),
                            title: title.into_string(),
                            buffer: String::new(),
                        });
                    }
                }
                Tag::Paragraph => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack).push_str("<p>");
                    }
                }
                Tag::Heading { level, .. } => {
                    if image_stack.is_empty() {
                        let tag_name = match level {
                            HeadingLevel::H1 => "h1",
                            HeadingLevel::H2 => "h2",
                            HeadingLevel::H3 => "h3",
                            HeadingLevel::H4 => "h4",
                            HeadingLevel::H5 => "h5",
                            HeadingLevel::H6 => "h6",
                        };
                        let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                        buf.push('<');
                        buf.push_str(tag_name);
                        buf.push('>');
                    }
                }
                Tag::BlockQuote(_) => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("<blockquote>");
                    }
                }
                Tag::CodeBlock(kind) => {
                    if image_stack.is_empty() {
                        let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                        match kind {
                            CodeBlockKind::Fenced(lang) if !lang.trim().is_empty() => {
                                let lang_escaped = escape_html(lang.trim());
                                buf.push_str("<pre class=\"md-code-block\"><code data-lang=\"");
                                buf.push_str(&lang_escaped);
                                buf.push_str("\">");
                            }
                            _ => {
                                buf.push_str("<pre class=\"md-code-block\"><code>");
                            }
                        }
                    }
                }
                Tag::HtmlBlock => {}
                Tag::List(None) => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack).push_str("<ul>");
                    }
                }
                Tag::List(Some(1)) => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack).push_str("<ol>");
                    }
                }
                Tag::List(Some(start)) => {
                    if image_stack.is_empty() {
                        let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                        buf.push_str(&format!("<ol start=\"{start}\">"));
                    }
                }
                Tag::Item => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack).push_str("<li>");
                    }
                }
                Tag::Emphasis => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack).push_str("<em>");
                    }
                }
                Tag::Strong => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("<strong>");
                    }
                }
                Tag::Strikethrough => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack).push_str("<del>");
                    }
                }
                Tag::Table(aligns) => {
                    if image_stack.is_empty() {
                        table_alignments = aligns;
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("<div class=\"md-table-wrap\"><table>");
                    }
                }
                Tag::TableHead => {
                    if image_stack.is_empty() {
                        in_table_head = true;
                        col_index = 0;
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("<thead><tr>");
                    }
                }
                Tag::TableRow => {
                    if image_stack.is_empty() {
                        col_index = 0;
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("<tr>");
                    }
                }
                Tag::TableCell => {
                    if image_stack.is_empty() {
                        let tag_name = if in_table_head { "th" } else { "td" };
                        let align_attr = match table_alignments.get(col_index) {
                            Some(Alignment::Left) => " style=\"text-align:left\"",
                            Some(Alignment::Center) => " style=\"text-align:center\"",
                            Some(Alignment::Right) => " style=\"text-align:right\"",
                            _ => "",
                        };
                        col_index += 1;
                        let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                        buf.push('<');
                        buf.push_str(tag_name);
                        buf.push_str(align_attr);
                        buf.push('>');
                    }
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Image => {
                    if let Some(frame) = image_stack.pop() {
                        let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                        let trimmed_alt = frame.alt_text.trim();
                        if trimmed_alt.is_empty() {
                            buf.push_str("<span class=\"md-image-alt\" role=\"img\">[画像]</span>");
                        } else {
                            buf.push_str("<span class=\"md-image-alt\" role=\"img\">[画像: ");
                            buf.push_str(&escape_html(trimmed_alt));
                            buf.push_str("]</span>");
                        }
                    }
                }
                TagEnd::Link => {
                    if image_stack.is_empty() {
                        if let Some(frame) = link_stack.pop() {
                            let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                            match validate_and_normalize_url(&frame.dest_url) {
                                Ok(norm) => {
                                    let escaped_url = escape_html(&norm.normalized_url);
                                    let title_attr = if frame.title.trim().is_empty() {
                                        escaped_url.clone()
                                    } else {
                                        format!(
                                            "{} ({escaped_url})",
                                            escape_html(frame.title.trim())
                                        )
                                    };
                                    buf.push_str(
                                        "<button type=\"button\" class=\"md-external-link\" data-external-url=\"",
                                    );
                                    buf.push_str(&escaped_url);
                                    buf.push_str("\" title=\"");
                                    buf.push_str(&title_attr);
                                    buf.push_str("\">");
                                    buf.push_str(&frame.buffer);
                                    buf.push_str(
                                        "<span class=\"md-link-arrow\" aria-hidden=\"true\">↗</span></button>",
                                    );
                                }
                                Err(_) => {
                                    buf.push_str(
                                        "<span class=\"md-invalid-link\" title=\"対象外または無効なURL\">",
                                    );
                                    buf.push_str(&frame.buffer);
                                    buf.push_str("</span>");
                                }
                            }
                        }
                    }
                }
                TagEnd::Paragraph => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</p>");
                    }
                }
                TagEnd::Heading(level) => {
                    if image_stack.is_empty() {
                        let tag_name = match level {
                            HeadingLevel::H1 => "h1",
                            HeadingLevel::H2 => "h2",
                            HeadingLevel::H3 => "h3",
                            HeadingLevel::H4 => "h4",
                            HeadingLevel::H5 => "h5",
                            HeadingLevel::H6 => "h6",
                        };
                        let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                        buf.push_str("</");
                        buf.push_str(tag_name);
                        buf.push('>');
                    }
                }
                TagEnd::BlockQuote(_) => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</blockquote>");
                    }
                }
                TagEnd::CodeBlock => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</code></pre>");
                    }
                }
                TagEnd::HtmlBlock => {}
                TagEnd::List(ordered) => {
                    if image_stack.is_empty() {
                        let closing = if ordered { "</ol>" } else { "</ul>" };
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str(closing);
                    }
                }
                TagEnd::Item => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</li>");
                    }
                }
                TagEnd::Emphasis => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</em>");
                    }
                }
                TagEnd::Strong => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</strong>");
                    }
                }
                TagEnd::Strikethrough => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</del>");
                    }
                }
                TagEnd::Table => {
                    if image_stack.is_empty() {
                        table_alignments.clear();
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</tbody></table></div>");
                    }
                }
                TagEnd::TableHead => {
                    if image_stack.is_empty() {
                        in_table_head = false;
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</tr></thead><tbody>");
                    }
                }
                TagEnd::TableRow => {
                    if image_stack.is_empty() {
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str("</tr>");
                    }
                }
                TagEnd::TableCell => {
                    if image_stack.is_empty() {
                        let closing = if in_table_head { "</th>" } else { "</td>" };
                        target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                            .push_str(closing);
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some(img) = image_stack.last_mut() {
                    img.alt_text.push_str(&text);
                } else {
                    target_buf(&mut root_html, &mut link_stack, &mut image_stack)
                        .push_str(&escape_html(&text));
                }
            }
            Event::Code(code) => {
                if let Some(img) = image_stack.last_mut() {
                    img.alt_text.push_str(&code);
                } else {
                    let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                    buf.push_str("<code>");
                    buf.push_str(&escape_html(&code));
                    buf.push_str("</code>");
                }
            }
            Event::Html(html) => {
                if let Some(img) = image_stack.last_mut() {
                    img.alt_text.push_str(&html);
                } else {
                    let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                    buf.push_str("<pre class=\"md-raw-html\"><code>");
                    buf.push_str(&escape_html(html.trim_end_matches('\n')));
                    buf.push_str("</code></pre>");
                }
            }
            Event::InlineHtml(html) => {
                if let Some(img) = image_stack.last_mut() {
                    img.alt_text.push_str(&html);
                } else {
                    let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                    buf.push_str("<span class=\"md-raw-html\">");
                    buf.push_str(&escape_html(&html));
                    buf.push_str("</span>");
                }
            }
            Event::SoftBreak => {
                if let Some(img) = image_stack.last_mut() {
                    img.alt_text.push(' ');
                } else {
                    target_buf(&mut root_html, &mut link_stack, &mut image_stack).push('\n');
                }
            }
            Event::HardBreak => {
                if let Some(img) = image_stack.last_mut() {
                    img.alt_text.push(' ');
                } else {
                    target_buf(&mut root_html, &mut link_stack, &mut image_stack).push_str("<br />");
                }
            }
            Event::Rule => {
                if image_stack.is_empty() {
                    target_buf(&mut root_html, &mut link_stack, &mut image_stack).push_str("<hr />");
                }
            }
            Event::TaskListMarker(checked) => {
                if image_stack.is_empty() {
                    let buf = target_buf(&mut root_html, &mut link_stack, &mut image_stack);
                    if checked {
                        buf.push_str(
                            "<input type=\"checkbox\" class=\"md-task-checkbox\" disabled checked tabindex=\"-1\" /> ",
                        );
                    } else {
                        buf.push_str(
                            "<input type=\"checkbox\" class=\"md-task-checkbox\" disabled tabindex=\"-1\" /> ",
                        );
                    }
                }
            }
            _ => {}
        }
    }

    root_html
}
