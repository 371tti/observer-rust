use std::str::FromStr;

use openai_dive::v1::resources::response::response::Role;
use serde_json::json;
use serenity::all::{
    Builder, ChannelId, ChannelType, CreateMessage, CreateThread, EditMessage, GetMessages, Message, MessageId, ReactionType
};

use crate::lmclient::LMTool;

pub struct DiscordTool;

impl DiscordTool {
    pub fn new() -> DiscordTool {
        DiscordTool {}
    }

    fn get_str_arg<'a>(args: &'a serde_json::Value, key: &'a str) -> Result<&'a str, String> {
        args.get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("Missing or invalid '{key}' parameter"))
    }
}

#[async_trait::async_trait]
impl LMTool for DiscordTool {
    fn name(&self) -> String {
        "discord-tool".to_string()
    }

    fn description(&self) -> String {
        "Interact with Discord: add/remove reactions, create threads, send/edit/fetch messages, and search messages in a channel.".to_string()
    }

    fn json_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "description": "Discord operation to perform.",
                    "enum": [
                        "add_reaction",
                        "remove_reaction",
                        "create_thread",
                        "send_message",
                        "edit_message",
                        "fetch_message",
                        "search_messages"
                    ]
                },
                "channel_id": {
                    "type": "string",
                    "description": "ID of the target channel. Required for all operations."
                },
                "message_id": {
                    "type": "string",
                    "description": "ID of the target message. Used by: add/remove_reaction, create_thread(from message), send_message(reply_to), edit_message, fetch_message."
                },
                "reaction": {
                    "type": "string",
                    "description": "Emoji for reactions. Unicode (e.g. 🫠,😱,👍,👈,🤔) or custom emoji ID. Used by: add_reaction, remove_reaction."
                },
                "name": {
                    "type": "string",
                    "description": "Name of the thread. Used by: create_thread."
                },
                "thread_type": {
                    "type": "string",
                    "description": "Type of the thread. 'public' or 'private'. Defaults to 'public'. Used by: create_thread.",
                    "enum": ["public", "private"]
                },
                "content": {
                    "type": "string",
                    "description": "Message content. Used by: send_message, edit_message."
                },
                "reply_to": {
                    "type": "string",
                    "description": "Message ID to reply to. Optional. Used by: send_message."
                },
                "query": {
                    "type": "string",
                    "description": "Keyword to search in message content. Used by: search_messages."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max number of recent messages to scan (1–100). Defaults to 50. Used by: search_messages."
                }
            },
            "required": ["operation", "channel_id"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ob_ctx: crate::context::ObserverContext,
    ) -> Result<String, String> {
        let operation = args
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing or invalid 'operation' parameter".to_string())?;

        let channel_id_str = Self::get_str_arg(&args, "channel_id")?;
        let channel_id =
            ChannelId::from_str(channel_id_str).map_err(|e| format!("Invalid 'channel_id': {e}"))?;

        let http = ob_ctx.discord_client.open().http.clone();

        match operation {
            // --------------------
            // Reaction: add
            // --------------------
            "add_reaction" => {
                let message_id_str = Self::get_str_arg(&args, "message_id")?;
                let reaction = Self::get_str_arg(&args, "reaction")?;

                let message_id = MessageId::from_str(message_id_str)
                    .map_err(|e| format!("Invalid 'message_id': {e}"))?;

                channel_id
                    .create_reaction(
                        http,
                        message_id,
                        ReactionType::Unicode(reaction.to_string()),
                    )
                    .await
                    .map_err(|e| format!("Failed to add reaction: {e}"))?;

                Ok(format!(
                    "Added reaction '{}' on channel_id='{}', message_id='{}'",
                    reaction, channel_id_str, message_id_str
                ))
            }

            // --------------------
            // Reaction: remove
            // --------------------
            "remove_reaction" => {
                let message_id_str = Self::get_str_arg(&args, "message_id")?;
                let reaction = Self::get_str_arg(&args, "reaction")?;

                let message_id = MessageId::from_str(message_id_str)
                    .map_err(|e| format!("Invalid 'message_id': {e}"))?;

                channel_id
                    .delete_reaction_emoji(
                        http,
                        message_id,
                        ReactionType::Unicode(reaction.to_string()),
                    )
                    .await
                    .map_err(|e| format!("Failed to remove reaction: {e}"))?;

                Ok(format!(
                    "Removed reaction '{}' on channel_id='{}', message_id='{}'",
                    reaction, channel_id_str, message_id_str
                ))
            }

            // --------------------
            // Thread: create
            // --------------------
            "create_thread" => {
                let name = Self::get_str_arg(&args, "name")?;

                let message_id_str_opt = args.get("message_id").and_then(|v| v.as_str());
                let thread_type_str = args
                    .get("thread_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("public");

                let channel_type = match thread_type_str {
                    "public" => ChannelType::PublicThread,
                    "private" => ChannelType::PrivateThread,
                    other => {
                        return Err(format!(
                            "Unsupported 'thread_type': {other}. Use 'public' or 'private'."
                        ));
                    }
                };

                let message_id_opt: Option<MessageId> = match message_id_str_opt {
                    Some(s) => {
                        let mid = MessageId::from_str(s)
                            .map_err(|e| format!("Invalid 'message_id': {e}"))?;
                        Some(mid)
                    }
                    None => None,
                };

                let builder = CreateThread::new(name).kind(channel_type);

                let res = builder
                    .execute(&http, (channel_id, message_id_opt))
                    .await
                    .map_err(|e| format!("Failed to create thread: {e}"))?;

                // Chat コンテキスト移動ロジックは元のまま
                let mut context = ob_ctx.chat_contexts.get_or_create(channel_id);
                context.add_text(
                    "The context has been moved to the newly created thread. You are now inside the thread you created.".to_string(),
                    Role::System,
                );
                ob_ctx.chat_contexts.marge(res.id, &context);
                ob_ctx.chat_contexts.set_enabled(res.id, true);

                Ok(format!(
                    "Created {thread_type_str} thread '{}' in channel_id='{}' (from message_id='{}')",
                    name,
                    channel_id_str,
                    message_id_str_opt.unwrap_or("-")
                ))
            }

            // --------------------
            // Send message
            // --------------------
            "send_message" => {
                let content = Self::get_str_arg(&args, "content")?;

                let reply_to_str = args.get("reply_to").and_then(|v| v.as_str());

                let mut builder = CreateMessage::new().content(content);

                if let Some(reply_id_str) = reply_to_str {
                    let reply_id = MessageId::from_str(reply_id_str).map_err(|e| {
                        format!("Invalid 'reply_to' message_id: {e}")
                    })?;
                    builder = builder.reference_message((channel_id, reply_id));
                }

                let msg = channel_id
                    .send_message(&http, builder)
                    .await
                    .map_err(|e| format!("Failed to send message: {e}"))?;

                let result = json!({
                    "status": "ok",
                    "operation": operation,
                    "channel_id": channel_id_str,
                    "message_id": msg.id.to_string(),
                    "content": msg.content,
                });

                Ok(result.to_string())
            }

            // --------------------
            // Edit message
            // --------------------
            "edit_message" => {
                let message_id_str = Self::get_str_arg(&args, "message_id")?;
                let content = Self::get_str_arg(&args, "content")?;

                let message_id = MessageId::from_str(message_id_str)
                    .map_err(|e| format!("Invalid 'message_id': {e}"))?;

                let builder = EditMessage::new().content(content);

                let msg = channel_id
                    .edit_message(&http, message_id, builder)
                    .await
                    .map_err(|e| format!("Failed to edit message: {e}"))?;

                let result = json!({
                    "status": "ok",
                    "operation": operation,
                    "channel_id": channel_id_str,
                    "message_id": message_id_str,
                    "content": msg.content,
                });

                Ok(result.to_string())
            }

            // --------------------
            // Fetch message
            // --------------------
            "fetch_message" => {
                let message_id_str = Self::get_str_arg(&args, "message_id")?;
                let message_id = MessageId::from_str(message_id_str)
                    .map_err(|e| format!("Invalid 'message_id': {e}"))?;

                let msg = channel_id
                    .message(&http, message_id)
                    .await
                    .map_err(|e| format!("Failed to fetch message: {e}"))?;

                let result = json!({
                    "status": "ok",
                    "operation": operation,
                    "channel_id": channel_id_str,
                    "message_id": message_id_str,
                    "author_id": msg.author.id.to_string(),
                    "author_name": msg.author.name,
                    "content": msg.content,
                    "timestamp": msg.timestamp.to_string(),
                });

                Ok(result.to_string())
            }

            // --------------------
            // Search messages
            // --------------------
            "search_messages" => {
                let query = Self::get_str_arg(&args, "query")?;

                let limit = args
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(50)
                    .min(100) as u8;

                let messages: Vec<Message> = channel_id
                    .messages(
                        &http,
                        GetMessages::new().limit(limit),
                    )
                    .await
                    .map_err(|e| format!("Failed to fetch messages: {e}"))?;

                let lower_query = query.to_lowercase();
                let matched: Vec<serde_json::Value> = messages
                    .into_iter()
                    .filter(|m| m.content.to_lowercase().contains(&lower_query))
                    .map(|m| {
                        json!({
                            "message_id": m.id.to_string(),
                            "author_id": m.author.id.to_string(),
                            "author_name": m.author.name,
                            "content": m.content,
                            "timestamp": m.timestamp.to_string(),
                        })
                    })
                    .collect();

                let result = json!({
                    "status": "ok",
                    "operation": operation,
                    "channel_id": channel_id_str,
                    "query": query,
                    "matched_count": matched.len(),
                    "messages": matched,
                });

                Ok(result.to_string())
            }

            other => Err(format!(
                "Unsupported 'operation': {other}. \
                 Use one of: add_reaction, remove_reaction, create_thread, \
                 send_message, edit_message, fetch_message, search_messages."
            )),
        }
    }
}
