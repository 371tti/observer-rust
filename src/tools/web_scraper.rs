/*!
Webスクレイピングツール (Rust版)

このツールは指定されたURLからHTMLページを取得し、ユーザーが指定する
CSSセレクターに基づいて、該当する要素のテキストやリンク情報を抽出します。

【対応モード】
- `reqwest`: 高速スクレイピング（JavaScript 不可）
- `playwright`: JavaScript 対応のスクレイピング（動的サイト向け）
- `auto`: 自動判定（CloudflareやJSが必要なら Playwright 使用）

【使い方の例】
以下のJSONパラメータを渡すと、指定したURLのページから「p, h1, h2, h3, h4, h5, h6, a」タグの内容とリンク情報が抽出され、JSON形式で返されます：
{
    "url": "https://example.com",
    "selector": "p, h1, h2, h3, h4, h5, h6, a",
    "mode": "auto"
}
*/

use call_agent::chat::{err, function::Tool};
use regex::Regex;
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{error::Error, sync::Arc};
use playwright::Playwright;
use tokio::{self, sync::Mutex};
use std::fmt;

const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024; // 5MB
const WHITELIST: [&str; 110] = [
    "application/json",
    "text/markdown",
    "text/plain",
    "text/csv",
    "application/javascript",
    "text/css",
    "text/x-rust",
    "text/x-python",
    "text/x-java-source",
    "text/x-c",
    "text/x-c++src",
    "text/x-go",
    "application/xml",
    "text/xml",
    "application/xhtml+xml",
    "application/x-httpd-php",
    "text/x-php",
    "text/javascript",
    "application/ecmascript",
    "text/x-shellscript",
    "application/x-sh",
    "text/x-ruby",
    "application/x-ruby",
    "text/x-perl",
    "application/x-perl",
    "text/x-sql",
    "application/sql",
    "text/x-scala",
    "text/x-erlang",
    "text/x-haskell",
    "text/x-cobol",
    "text/x-fortran",
    "application/x-latex",
    "text/x-latex",
    "application/x-sqlite3",
    "application/atom+xml",
    "application/rss+xml",
    "application/vnd.api+json",
    "application/x-yaml",
    "application/ld+json",
    "text/vnd.graphviz",
    "text/x-csh",
    "application/typescript",
    "text/x-d",
    "text/x-swift",
    "text/x-kotlin",
    "text/x-objective-c",
    "text/x-pascal",
    "text/x-vb",
    "text/x-r",
    "text/x-dart",
    "application/x-prolog",
    "text/x-prolog",
    "text/x-asciidoc",
    "text/x-org",
    "application/json5",
    "text/x-sqlite",
    "application/x-tex",
    "text/x-tex",
    "application/x-bibtex",
    "text/x-bibtex",
    "text/rtf",
    "application/edn",
    "text/x-clojure",
    "application/x-clojure",
    "text/x-lisp",
    "application/x-lisp",
    "text/x-config",
    "text/x-env",
    "text/x-applescript",
    "text/x-scm",
    "text/x-rst",
    "application/x-powershell",
    "text/x-powershell",
    "text/x-vhdl",
    "text/x-verilog",
    "text/x-vue",
    "text/x-svelte",
    "text/x-coffeescript",
    "text/x-lua",
    "application/x-lua",
    "text/x-rpm-spec",
    "text/x-dockerfile",
    "text/x-ini",
    "text/x-properties",
    "text/x-toml",
    "application/x-toml",
    "text/x-xslt",
    "application/xml-dtd",
    "text/x-json",
    "application/x-json",
    "text/x-cmake",
    "text/x-diff",
    "text/x-log",
    "text/x-nsis",
    "text/x-asm",
    "text/x-lilypond",
    "text/x-llvm",
    "text/x-cl",
    "text/x-tcl",
    "application/x-tcl",
    "text/x-puppet",
    "application/x-puppet",
    "text/x-nim",
    "text/x-zig",
    "text/x-crystal",
    "text/x-fsharp",
    "text/x-vbscript",
    "text/x-msdos-batch",
    "text/x-awk",
];

#[derive(Debug)]
pub enum ScraperError {
    NetworkError,
    ParseError,
    TimeoutError,
    FileTooLargeError,
    InitializationError,
    ContextError,
    LaunchError,
    PageError,
    ScriptError,
    UnknownError,
}

impl fmt::Display for ScraperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScraperError::NetworkError => write!(f, "Network error occurred."),
            ScraperError::ParseError => write!(f, "Error occurred while parsing data."),
            ScraperError::TimeoutError => write!(f, "Request timed out."),
            ScraperError::FileTooLargeError => write!(f, "File size is too large."),
            ScraperError::InitializationError => write!(f, "Failed to initialize Playwright."),
            ScraperError::ContextError => write!(f, "Failed to create Playwright context."),
            ScraperError::LaunchError => write!(f, "Failed to launch Playwright browser."),
            ScraperError::PageError => write!(f, "Failed to create Playwright page."),
            ScraperError::ScriptError => write!(f, "Failed to add Playwright script."),
            ScraperError::UnknownError => write!(f, "An unknown error occurred."),
        }
    }
}

impl std::error::Error for ScraperError {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapedItem {
    pub text: String,  // 要素内のテキスト
    pub link: Option<String>,  // リンクの href 属性
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapedData {
    pub items: Vec<ScrapedItem>,
}

#[derive(Clone)]
pub struct WebScraper {
    client: Client,
}

impl WebScraper {
    /// 新しいWebScraperインスタンスを生成する
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (compatible; WebScraper/1.0)")
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to build reqwest client");
        WebScraper { client }
    }

    /// 通常の HTTP スクレイピング（reqwest）
    pub async fn scrape_reqwest(
        &self,
        url: &str,
        selector_str: &str,
    ) -> Result<ScrapedData, ScraperError> {
        // 5MBを上限とする
        let url = Url::parse(url).map_err(|_| ScraperError::ParseError)?;

        let response = self.client.get(url)
            .send()
            .await
            .map_err(|_| ScraperError::NetworkError)?;

        // ヘッダーにContent-Lengthがある場合、サイズをチェックする
        if let Some(len) = response.content_length() {
            if len > MAX_FILE_SIZE {
                return Err(ScraperError::FileTooLargeError);
            }
        }

        let content_type = response.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let response = response.error_for_status().map_err(|_| ScraperError::NetworkError)?;

        let body_bytes = response.bytes().await.map_err(|_| ScraperError::NetworkError)?;
        if body_bytes.len() > MAX_FILE_SIZE as usize {
            return Err(ScraperError::FileTooLargeError);
        }
    
        // UTF-8 でデコードできない場合はエラーを返す
        let body = String::from_utf8(body_bytes.to_vec()).map_err(|_| ScraperError::ParseError)?;

        if !content_type.contains("text/html") {
            if WHITELIST.iter().any(|&item| content_type.contains(item)) {
                return Ok(ScrapedData {
                    items: vec![ScrapedItem {
                        text: body,
                        link: None,
                    }],
                });
            }    
            return Err(ScraperError::ParseError);
        }
        
        let document = Html::parse_document(&body);
        let selector = Selector::parse(selector_str).map_err(|_| ScraperError::ParseError)?;

        let items: Vec<ScrapedItem> = document
            .select(&selector)
            .map(|element| {
                let raw_text = element.text().collect::<Vec<_>>().join(" ");
                let text = raw_text.split_whitespace().collect::<String>();

                let href = element.value().attr("href").map(|s| s.to_string());
                let link = element.value().attr("src").map(|s| s.to_string());

                ScrapedItem { text, link: href.or(link) }
            })
            .filter(|item| !item.text.is_empty() || item.link.is_some())
            .collect();

        Ok(ScrapedData { items })
    }


    /// Playwright を使った JS レンダリング対応スクレイピング
    pub async fn scrape_playwright(
        &self,
        url: &str,
        selector_str: &str,
    ) -> Result<ScrapedData, ScraperError> {
        let playwright = Playwright::initialize().await.map_err(|_| ScraperError::InitializationError)?;
        let browser = playwright.chromium().launcher().headless(true).launch().await.map_err(|_| ScraperError::LaunchError)?;
        let context = browser.context_builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko, OBSERVER SCRAPER) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .await.map_err(|_| ScraperError::ContextError)?;

        let page = context.new_page().await.map_err(|_| ScraperError::PageError)?;
        page.add_init_script(include_str!("stealth.min.js")).await.map_err(|_| ScraperError::ScriptError)?;
        page.goto_builder(url).timeout(10000.0).goto().await.map_err(|_| ScraperError::NetworkError)?;
        page.wait_for_selector_builder(selector_str).wait_for_selector().await.map_err(|_| ScraperError::TimeoutError)?;
        let elements = page.eval(
            &format!("Array.from(document.querySelectorAll('{}')).map(e => ({{\
                text: e.innerText || '',
                href: e.getAttribute('href') || '',
                src: e.getAttribute('src') || ''
            }}))", selector_str)
        ).await.map_err(|_| ScraperError::ParseError)?;

        let items: Vec<ScrapedItem> = serde_json::from_value(elements).map_err(|_| ScraperError::ParseError)?;

        Ok(ScrapedData { items })
    }

    pub fn compress_content(content: ScrapedData, seek_pos: usize, len: usize) -> String {
        let mut combined_text = String::new();
    
        // すべてのテキストとリンクをまとめる
        for item in content.items {
            combined_text.push_str(&item.text);
    
            if let Some(link) = item.link {
                combined_text.push_str(&format!("({})", link));
            }
        }
    
        let total_chars = combined_text.chars().count(); // 全体の文字数
    
        // seek_posが文字数を超えていたら空文字を返す
        if seek_pos >= total_chars {
            return format!("...<0 characters remaining>");
        }
    
        // seek_posから取得可能な文字数
        let available_chars = total_chars - seek_pos;
        
        // 切り出す範囲を計算
        let sliced_text: String = combined_text
            .chars()
            .skip(seek_pos)
            .take(len)
            .collect();
    
        // 残り文字数を正しく計算
        let remaining_chars = available_chars.saturating_sub(sliced_text.chars().count());
    
        format!("{}...<{} characters remaining>", sliced_text, remaining_chars)
    }
    
}

/// AI Functionとして利用するための `Tool` トレイト実装
impl Tool for WebScraper {
    fn def_name(&self) -> &str {
        "web_scraper"
    }

    fn def_description(&self) -> &str {
        "Extracts webpage content using a CSS selector (avoid '*', use specific tags like 'p, h1, h2, h3, a').  
        Supports 'reqwest' (fast) and 'playwright' (beta, may be unstable) for JavaScript-heavy pages.  
        Use 'seek_pos' and 'max_length' to paginate (e.g., 0-2000, 2000-4000) for full extraction.  
        **If no content is retrieved, consider:**
        - The site may require JavaScript rendering ('playwright' mode).
        - The selector may be incorrect.
        - The site may block scraping.  
        **Always include the scraped URL at the end of your response.**"
    }

    fn def_parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "Target webpage URL (e.g., 'https://example.com')."
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector to extract elements(ex., 'p, h1, h2, h3, a, img, video, audio, image...') "
                },
                "mode": {
                    "type": "string",
                    "enum": ["reqwest", "playwright"],
                    "description": "Scraping method: 'reqwest' (fast) or 'playwright' (e.g., 'reqwest')."
                },
                "seek_pos": {
                    "type": "integer",
                    "description": "Character position to start extracting content (e.g., 0)."
                },
                "max_length": {
                    "type": "integer",
                    "description": "Maximum length of extracted content (e.g., 1000)."
                }
            },
            "required": ["url", "selector", "seek_pos", "max_length"]
        })
    }
    

    fn run(&self, args: serde_json::Value) -> Result<String, String> {
        let url = args.get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'url' parameter".to_string())?
            .to_string();

        let selector = args.get("selector")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing 'selector' parameter".to_string())?
            .to_string();

        let mode = args.get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("reqwest")
            .to_string();

        let seek_pos = args.get("seek_pos")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "Missing 'seek_pos' parameter".to_string())? as usize;

        let max_length = args.get("max_length")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "Missing 'max_length' parameter".to_string())? as usize;

        let scraper = self.clone();

        let result = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            match mode.as_str() {
                "reqwest" => rt.block_on(scraper.scrape_reqwest(&url, &selector))
                    .or_else(|_| rt.block_on(scraper.scrape_playwright(&url, &selector))),
                "playwright" => rt.block_on(scraper.scrape_playwright(&url, &selector)),
                _ => Err(ScraperError::UnknownError),
            }
        })
        .join()
        .map_err(|_| "Thread panicked".to_string())?
        .map_err(|e| format!("Scrape error: {}", e))?;

        let res = WebScraper::compress_content(result, seek_pos, max_length);
        serde_json::to_string(&res).map_err(|e| format!("Serialization error: {}", e))
    }
}
