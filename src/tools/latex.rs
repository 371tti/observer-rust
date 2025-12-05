use std::str::FromStr;

use serde_json::json;
use serenity::all::{ChannelId, CreateAttachment, CreateMessage, MessageId};
use wk_371tti_net_crawler::CaptureAPIBuilder;

use crate::{context::ObserverContext, lmclient::LMTool};

pub struct LatexExprRenderTool;

impl LatexExprRenderTool {
    pub fn new() -> LatexExprRenderTool {
        LatexExprRenderTool {}
    }

    pub async fn render(expr: &str, ob_ctx: &ObserverContext) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("http://{host}:{port}/latex_expr_render#{expr}",
            host=ob_ctx.config.web_server_local_ip.iter().map(|b| b.to_string()).collect::<Vec<String>>().join("."),
            port=ob_ctx.config.web_server_port,
            expr=expr
        );
        let png = ob_ctx.scraper.capture_api(
            CaptureAPIBuilder::new(&url)
                .set_selector(".capture")
                .set_wait_millis(200)
                .build()
        ).await;
        png
    }
}

#[async_trait::async_trait]
impl LMTool for LatexExprRenderTool {
    fn name(&self) -> String {
        "latex_expr_render".to_string()
    }

    fn description(&self) -> String {
        "Render LaTeX expressions to images and send to Discord.".to_string()
    }

    fn json_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "channel_id": {
                    "type": "string",
                    "description": "ID of the target channel on Discord."
                },
                "reply_to": {
                    "type": "string",
                    "description": "Optional message ID to reply to."
                },
                "expression": {
                    "type": "string",
                    "description": "The LaTeX expression to render."
                }
            },
            "required": ["expression", "channel_id"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ob_ctx: crate::context::ObserverContext,
    ) -> Result<String, String> {
        // --- 引数パース ---
        let channel_id_str = args
            .get("channel_id")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'channel_id'".to_string())?;

        let expr = args
            .get("expression")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'expression'".to_string())?;

        let reply_to = args
            .get("reply_to")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        let channel_id = ChannelId::from_str(channel_id_str)
            .map_err(|e| format!("Invalid 'channel_id': {e}"))?;

        let reply_message_id = if let Some(id_str) = reply_to {
            Some(
                MessageId::from_str(id_str)
                    .map_err(|e| format!("Invalid 'reply_to' message id: {e}"))?,
            )
        } else {
            None
        };

        // --- LaTeX → 画像レンダリング ---
        let png_bytes = Self::render(expr, &ob_ctx)
            .await
            .map_err(|e| format!("Failed to render LaTeX expression: {e}"))?;

        // --- Discord 送信 ---
        let http = ob_ctx.discord_client.open().http.clone();

        let attachment = CreateAttachment::bytes(png_bytes, "latex.png");

        let mut builder = CreateMessage::new()
            .add_file(attachment);

        if let Some(msg_id) = reply_message_id {
            // (ChannelId, MessageId) から MessageReference を作る From 実装がある
            builder = builder.reference_message((channel_id, msg_id));
        }

        let msg = channel_id
            .send_message(&http, builder)
            .await
            .map_err(|e| format!("Failed to send Discord message: {e}"))?;

        let result = json!({
            "status": "ok",
            "message_id": msg.id.to_string(),
            "channel_id": channel_id.to_string(),
            "expression": expr,
        });

        Ok(result.to_string())
    }
}