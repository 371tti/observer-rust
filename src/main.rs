use std::sync::Arc;

use call_agent::{client::{ModelConfig, OpenAIClient}, function::Tool, prompt::{Message, MessageContext, MessageImage}};
use observer::brain::tools::{get_time::GetTime, memory::MemoryTool, web_scraper::WebScraper, www_search::WebSearch};
use serde_json::Value;

/// **テキストの長さを計算するツール**
pub struct TextLengthTool;

impl TextLengthTool {
    pub fn new() -> Self {
        Self
    }
}

impl Tool for TextLengthTool {
    fn def_name(&self) -> &str {
        "text_length_tool"
    }

    fn def_description(&self) -> &str {
        "Returns the length of the input text."
    }

    fn def_parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Input text to calculate its length"
                }
            },
            "required": ["text"]
        })
    }

    fn run(&self, args: Value) -> Result<String, String> {
        println!("{:?}", args);
        // JSONから"text"キーを取得
        let text = args["text"].as_str()
            .ok_or_else(|| "Missing 'text' parameter".to_string())?;
        
        // 長さを計算
        let length = text.len();

        // JSONで結果を返す
        Ok(serde_json::json!({ "length": length }).to_string())
    }
}



#[tokio::main]
async fn main() {
   
    c.def_tool(Arc::new(TextLengthTool::new()));
    c.def_tool(Arc::new(GetTime::new()));
    //c.def_tool(Arc::new(WebSearch::new()));
    c.def_tool(Arc::new(WebScraper::new()));
    c.def_tool(Arc::new(MemoryTool::new()));

    let conf = ModelConfig{
        model: "gpt-4o-mini".to_string(),
        temp: Some(0.5),
        max_token: Some(4000),
        top_p: Some(1.0),
    };

    
    let mut prompt_stream = c.create_prompt();
    loop {
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");
        let prompt = vec![Message::User {
            content: vec![MessageContext::Text(input.trim().to_string())],
        }];
        prompt_stream.add(prompt).await;

        loop {
            // Ask for a continuation or function response
            let _ = prompt_stream.generate_use_tool(&conf).await;
            let res = prompt_stream.last().await.unwrap();

            match res {
                // If the response comes from a tool, continue generating.
                Message::Function { .. } => continue,
                // When we have a plain response, try to extract its text and print it.
                Message::Assistant { ref content, .. } => {
                    if let Some(MessageContext::Text(text)) = content.first() {
                        // Replace escape sequences with actual newlines
                        let formatted_text = text.replace("\\n", "\n");
                        println!("{}", formatted_text);
                    } else {
                        println!("{:?}", res);
                    }
                    break;
                }
                // Fallback for any other message type.
                _ => {
                    println!("{:?}", res);
                    break;
                }
            }
        }
    }

}
