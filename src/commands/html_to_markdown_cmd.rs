use async_trait::async_trait;
use regex_lite::Regex;
use crate::commands::{Command, CommandContext, CommandResult};

pub struct HtmlToMarkdownCommand;

#[async_trait]
impl Command for HtmlToMarkdownCommand {
    fn name(&self) -> &'static str { "html-to-markdown" }

    async fn execute(&self, ctx: CommandContext) -> CommandResult {
        if ctx.args.iter().any(|a| a == "--help") {
            return CommandResult::success(
                "html-to-markdown - convert HTML to Markdown\n\nUsage: html-to-markdown [FILE]\n\nConvert HTML to Markdown format.\n".to_string()
            );
        }

        let input = if ctx.args.is_empty() || ctx.args[0] == "-" {
            ctx.stdin.clone()
        } else {
            let path = ctx.fs.resolve_path(&ctx.cwd, &ctx.args[0]);
            match ctx.fs.read_file(&path).await {
                Ok(c) => c,
                Err(_) => return CommandResult::error(
                    format!("html-to-markdown: {}: No such file or directory\n", ctx.args[0])
                ),
            }
        };

        if input.trim().is_empty() {
            return CommandResult::success(String::new());
        }

        let markdown = html_to_markdown(&input);
        CommandResult::success(format!("{}\n", markdown.trim()))
    }
}

fn html_to_markdown(html: &str) -> String {
    let mut result = html.to_string();

    let script_re = Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap();
    result = script_re.replace_all(&result, "").to_string();

    let style_re = Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap();
    result = style_re.replace_all(&result, "").to_string();

    let comment_re = Regex::new(r"(?s)<!--.*?-->").unwrap();
    result = comment_re.replace_all(&result, "").to_string();

    for i in 1..=6 {
        let hashes = "#".repeat(i);
        let h_re = Regex::new(&format!(r"(?is)<h{}[^>]*>(.*?)</h{}>", i, i)).unwrap();
        result = h_re.replace_all(&result, |caps: &regex_lite::Captures| {
            format!("{} {}\n\n", hashes, strip_tags(caps.get(1).map_or("", |m| m.as_str())).trim())
        }).to_string();
    }

    let strong_re = Regex::new(r"(?is)<strong[^>]*>(.*?)</strong>").unwrap();
    result = strong_re.replace_all(&result, |caps: &regex_lite::Captures| {
        format!("**{}**", strip_tags(caps.get(1).map_or("", |m| m.as_str())))
    }).to_string();

    let b_re = Regex::new(r"(?is)<b[^>]*>(.*?)</b>").unwrap();
    result = b_re.replace_all(&result, |caps: &regex_lite::Captures| {
        format!("**{}**", strip_tags(caps.get(1).map_or("", |m| m.as_str())))
    }).to_string();

    let em_re = Regex::new(r"(?is)<em[^>]*>(.*?)</em>").unwrap();
    result = em_re.replace_all(&result, |caps: &regex_lite::Captures| {
        format!("_{}_", strip_tags(caps.get(1).map_or("", |m| m.as_str())))
    }).to_string();

    let i_re = Regex::new(r"(?is)<i[^>]*>(.*?)</i>").unwrap();
    result = i_re.replace_all(&result, |caps: &regex_lite::Captures| {
        format!("_{}_", strip_tags(caps.get(1).map_or("", |m| m.as_str())))
    }).to_string();

    let code_re = Regex::new(r"(?is)<code[^>]*>(.*?)</code>").unwrap();
    result = code_re.replace_all(&result, |caps: &regex_lite::Captures| {
        format!("`{}`", caps.get(1).map_or("", |m| m.as_str()))
    }).to_string();

    let pre_re = Regex::new(r"(?is)<pre[^>]*>(.*?)</pre>").unwrap();
    result = pre_re.replace_all(&result, |caps: &regex_lite::Captures| {
        format!("```\n{}\n```\n", strip_tags(caps.get(1).map_or("", |m| m.as_str())).trim())
    }).to_string();

    let a_re = Regex::new(r#"(?is)<a[^>]*href=["']([^"']+)["'][^>]*>(.*?)</a>"#).unwrap();
    result = a_re.replace_all(&result, |caps: &regex_lite::Captures| {
        let href = caps.get(1).map_or("", |m| m.as_str());
        let text = strip_tags(caps.get(2).map_or("", |m| m.as_str()));
        format!("[{}]({})", text.trim(), href)
    }).to_string();

    let img_re = Regex::new(r#"(?is)<img[^>]*src=["']([^"']+)["'][^>]*alt=["']([^"']*)["'][^>]*/?>"#).unwrap();
    result = img_re.replace_all(&result, |caps: &regex_lite::Captures| {
        let src = caps.get(1).map_or("", |m| m.as_str());
        let alt = caps.get(2).map_or("", |m| m.as_str());
        format!("![{}]({})", alt, src)
    }).to_string();

    let img_re2 = Regex::new(r#"(?is)<img[^>]*src=["']([^"']+)["'][^>]*/?>"#).unwrap();
    result = img_re2.replace_all(&result, |caps: &regex_lite::Captures| {
        let src = caps.get(1).map_or("", |m| m.as_str());
        format!("![]({})", src)
    }).to_string();

    let li_re = Regex::new(r"(?is)<li[^>]*>(.*?)</li>").unwrap();
    result = li_re.replace_all(&result, |caps: &regex_lite::Captures| {
        format!("- {}\n", strip_tags(caps.get(1).map_or("", |m| m.as_str())).trim())
    }).to_string();

    let p_re = Regex::new(r"(?is)<p[^>]*>(.*?)</p>").unwrap();
    result = p_re.replace_all(&result, |caps: &regex_lite::Captures| {
        format!("{}\n\n", strip_tags(caps.get(1).map_or("", |m| m.as_str())).trim())
    }).to_string();

    let br_re = Regex::new(r"(?i)<br\s*/?>").unwrap();
    result = br_re.replace_all(&result, "\n").to_string();

    let hr_re = Regex::new(r"(?i)<hr\s*/?>").unwrap();
    result = hr_re.replace_all(&result, "\n---\n\n").to_string();

    let blockquote_re = Regex::new(r"(?is)<blockquote[^>]*>(.*?)</blockquote>").unwrap();
    result = blockquote_re.replace_all(&result, |caps: &regex_lite::Captures| {
        let content = strip_tags(caps.get(1).map_or("", |m| m.as_str()));
        content.lines().map(|l| format!("> {}", l.trim())).collect::<Vec<_>>().join("\n") + "\n\n"
    }).to_string();

    result = strip_tags(&result);

    let entity_re = Regex::new(r"&nbsp;").unwrap();
    result = entity_re.replace_all(&result, " ").to_string();
    let entity_re = Regex::new(r"&lt;").unwrap();
    result = entity_re.replace_all(&result, "<").to_string();
    let entity_re = Regex::new(r"&gt;").unwrap();
    result = entity_re.replace_all(&result, ">").to_string();
    let entity_re = Regex::new(r"&amp;").unwrap();
    result = entity_re.replace_all(&result, "&").to_string();
    let entity_re = Regex::new(r"&quot;").unwrap();
    result = entity_re.replace_all(&result, "\"").to_string();

    let multi_newline = Regex::new(r"\n{3,}").unwrap();
    result = multi_newline.replace_all(&result, "\n\n").to_string();

    result.trim().to_string()
}

fn strip_tags(html: &str) -> String {
    let tag_re = Regex::new(r"<[^>]+>").unwrap();
    tag_re.replace_all(html, "").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::fs::InMemoryFs;

    fn create_ctx(args: Vec<&str>) -> CommandContext {
        CommandContext {
            args: args.into_iter().map(String::from).collect(),
            stdin: String::new(),
            cwd: "/".to_string(),
            env: HashMap::new(),
            fs: Arc::new(InMemoryFs::new()),
            exec_fn: None,
            fetch_fn: None,
        }
    }

    #[tokio::test]
    async fn test_help() {
        let ctx = create_ctx(vec!["--help"]);
        let result = HtmlToMarkdownCommand.execute(ctx).await;
        assert!(result.stdout.contains("html-to-markdown"));
        assert!(result.stdout.contains("Markdown"));
    }

    #[tokio::test]
    async fn test_empty_input() {
        let ctx = create_ctx(vec![]);
        let result = HtmlToMarkdownCommand.execute(ctx).await;
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.is_empty());
    }

    #[tokio::test]
    async fn test_headings() {
        let mut ctx = create_ctx(vec![]);
        ctx.stdin = "<h1>Title</h1><h2>Subtitle</h2>".to_string();
        let result = HtmlToMarkdownCommand.execute(ctx).await;
        assert!(result.stdout.contains("# Title"));
        assert!(result.stdout.contains("## Subtitle"));
    }

    #[tokio::test]
    async fn test_bold_italic() {
        let mut ctx = create_ctx(vec![]);
        ctx.stdin = "<strong>bold</strong> and <em>italic</em>".to_string();
        let result = HtmlToMarkdownCommand.execute(ctx).await;
        assert!(result.stdout.contains("**bold**"));
        assert!(result.stdout.contains("_italic_"));
    }

    #[tokio::test]
    async fn test_links() {
        let mut ctx = create_ctx(vec![]);
        ctx.stdin = r#"<a href="https://example.com">link</a>"#.to_string();
        let result = HtmlToMarkdownCommand.execute(ctx).await;
        assert!(result.stdout.contains("[link](https://example.com)"));
    }

    #[tokio::test]
    async fn test_code() {
        let mut ctx = create_ctx(vec![]);
        ctx.stdin = "<code>inline</code> and <pre>block</pre>".to_string();
        let result = HtmlToMarkdownCommand.execute(ctx).await;
        assert!(result.stdout.contains("`inline`"));
        assert!(result.stdout.contains("```"));
    }

    #[tokio::test]
    async fn test_list() {
        let mut ctx = create_ctx(vec![]);
        ctx.stdin = "<ul><li>one</li><li>two</li></ul>".to_string();
        let result = HtmlToMarkdownCommand.execute(ctx).await;
        assert!(result.stdout.contains("- one"));
        assert!(result.stdout.contains("- two"));
    }

    #[tokio::test]
    async fn test_strip_script_style() {
        let mut ctx = create_ctx(vec![]);
        ctx.stdin = "<script>alert(1)</script><style>.x{}</style><p>text</p>".to_string();
        let result = HtmlToMarkdownCommand.execute(ctx).await;
        assert!(!result.stdout.contains("alert"));
        assert!(!result.stdout.contains(".x"));
        assert!(result.stdout.contains("text"));
    }

    #[test]
    fn test_html_to_markdown_fn() {
        assert_eq!(html_to_markdown("<p>hello</p>").trim(), "hello");
        assert_eq!(html_to_markdown("<h1>title</h1>").trim(), "# title");
    }

    #[test]
    fn test_strip_tags_fn() {
        assert_eq!(strip_tags("<p>hello</p>"), "hello");
        assert_eq!(strip_tags("<a href='x'>link</a>"), "link");
    }
}
