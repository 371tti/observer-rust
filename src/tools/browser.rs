use log::info;
use wk_371tti_net_crawler::{ScraperAPIBuilder, schema::ScraperResult};

use crate::{context::ObserverContext, lmclient::LMTool};

pub struct Browser {}

impl Browser {
    pub fn new() -> Browser {
        Browser {}
    }
}

#[async_trait::async_trait]
impl LMTool for Browser {
    fn json_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL of the webpage to browse."
                },
                "with_links": {
                    "type": "boolean",
                    "description": "Whether to follow links on the page.",
                    "default": false
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector to extract specific content from the page."
                }
            },
            "required": ["url", "with_links"]
        })
    }

    fn description(&self) -> String {
        "Browse a webpage and extract content based on a CSS selector.".to_string()
    }

    fn name(&self) -> String {
        "browser".to_string()
    }

    async fn execute(&self, args: serde_json::Value, ob_ctx: ObserverContext) -> Result<String, String> {
        info!("Browser::execute called with args: {:?}", args);
        let url = args.get("url")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'url' parameter".to_string())?;
        let selector = args.get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let with_links = args.get("with_links")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let result = ob_ctx.scraper.scraper(
            ScraperAPIBuilder::new(url).set_text_selector(selector).build()
        ).await;

        match result {
            Ok(scraper_result) => {
                match scraper_result {
                    ScraperResult::Success { status, url, results } => {
                        let text = results.text;
                        let links = results.links;
                        if with_links {
                            // リンクも含めて返す
                            Ok(format!("Status: {}\nURL: {}\nExtracted Content:\n{}\nLinks:\n{:?}", status, url, text, links))
                        } else {
                            // テキストのみ返す
                            Ok(format!("Status: {}\nURL: {}\nExtracted Content:\n{}", status, url, text))
                        }
                    },
                    ScraperResult::Failed { error } => Err(format!("Scraper failed: {}", error)),
                }
            }
            Err(e) => Err(format!("Error during browsing: {}", e)),
        }
    }
}