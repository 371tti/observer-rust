use std::{collections::HashMap, sync::Arc, u64};

use call_agent::chat::{client::{OpenAIClient, OpenAIClientState, ToolMode}, prompt::{Message, MessageContext, MessageImage}};
use log::debug;
use observer::prefix::{ASK_DEVELOPER_PROMPT, ASSISTANT_NAME, MAX_USE_TOOL_COUNT};
use regex::Regex;
use serenity::all::{Context, CreateMessage, MessageFlags};
use tokio::sync::Mutex;

use crate::fetch_and_encode_images;

#[derive(Clone, Debug)]
pub struct InputMessage {
    pub content: String,
    pub name: String,
    pub message_id: String,
    pub reply_msg: Option<String>,
    pub user_id: String,
    pub attached_files: Vec<String>,
}
// 各チャンネルの会話履歴（state）を保持する構造体
pub struct ChannelState {
    // 並列処理のため、prompt_stream を Mutex で保護する
    pub prompt_stream: Mutex<OpenAIClientState>,
}

impl ChannelState {
    pub async fn new(client: &Arc<OpenAIClient>) -> Self {
        // 新しい PromptStream を生成する
        let mut prompt_stream = client.create_prompt();
        prompt_stream.set_entry_limit(64).await;
        // Extend lifetime to 'static; safe because client lives for the entire duration of the program
        Self {
            prompt_stream: Mutex::new(prompt_stream),
        }
    }

    async fn prepare_user_prompt(message: &mut InputMessage, viw_image_detail: u8) -> Vec<Message> {
        // スポイラーを含むメッセージの処理
        let re = Regex::new(r"(\|\|.*?\|\|)").unwrap();
        message.content = re.replace_all(&message.content, "||<spoiler_msg>||").to_string();

        // !hidetail が含まれていれば強制的に high detail
        let mut detail_flag = viw_image_detail;
        if message.content.contains("!hidetail") {
            println!("hidetail found in message content");
            detail_flag = 255;
            // 末尾／文中のフラグ文字列を削除
            message.content = message.content.replace("!hidetail", "");
        }

        let meta = format!(
            "[META]msg_id:{},user_name:{},replay_msg:{};\n{}",
            message.message_id,
            message.name,
            message.reply_msg.clone().unwrap_or_else(|| "none".into()),
            message.content.clone(),
        );

        let mut content_vec = Vec::new();
        content_vec.push(MessageContext::Text(meta));

        // detail_flag に応じて画像を追加
        if detail_flag != 0 {
            // 画像を取得して data URL にした Vec<String>
            let img_urls = fetch_and_encode_images(&message.attached_files).await;

            for url in img_urls {
                let detail_str = match detail_flag {
                    1 => Some("low".to_string()),
                    255 => Some("high".to_string()),
                    _ => None,
                };
                content_vec.push(MessageContext::Image(MessageImage {
                    url,
                    detail: detail_str,
                }));
            }
        }

        vec![Message::User {
            content: content_vec,
            name: Some(message.user_id.clone()),
        }]
    }

    pub async fn reasoning(
        &self, 
        ctx: &Context,
        msg: &serenity::all::Message,
        mut message: InputMessage,
    ) -> String {
        // プロンプトストリームの取得
        let user_prompt = ChannelState::prepare_user_prompt(&mut message, 1).await;
        let mut r_prompt_stream = self.prompt_stream.lock().await;
        r_prompt_stream.add(user_prompt).await;
        let mut prompt_stream = r_prompt_stream.clone();
        drop(r_prompt_stream); // 先にロックを解除t_stream.clone();
        prompt_stream.set_entry_limit(u64::MAX).await;
        let last_pos = prompt_stream.prompt.len();

        // システムプロンプトの追加
        debug!("prompt_stream - {:#?}", prompt_stream.prompt);
        let system_prompt = vec![Message::Developer {
            content: ASK_DEVELOPER_PROMPT.to_string(),
            name: Some(ASSISTANT_NAME.to_string()),
        }];
        prompt_stream.add(system_prompt).await;

        // 使用したツールのトラッキング
        let mut used_tools = Vec::new();

        // 推論ストリームの生成
        let mut reasoning_stream = match prompt_stream.reasoning(None, &ToolMode::Auto).await {
            Ok(stream) => stream,
            Err(e) => return format!("Err: failed reasoning - {:?}", e),
        };

        // 推論ループ
        for i in 0..*MAX_USE_TOOL_COUNT + 1 {
            // 終了できるなら終了
            if reasoning_stream.can_finish() {
                break;
            }
            
            // ツールコールの表示
            let show_tool_call: Vec<(String, serde_json::Value)> = reasoning_stream.show_tool_calls()
                .into_iter()
                .map(|(n,arg)| (n.to_string(), arg.clone()))
                .collect();

            for (tool_name, argument) in show_tool_call {
                used_tools.push(tool_name.clone());
                if let Some(explain) = argument.get("$explain") {
                    let status_res = CreateMessage::new()
                        .content(format!("-# {}...", explain.to_string()))
                        .flags(MessageFlags::SUPPRESS_EMBEDS);
    
                    if let Err(e) = msg.channel_id.send_message(&ctx.http, status_res).await {
                        debug!("Error sending message: {:?}", e);
                    }
                } else {
                    let status_res = CreateMessage::new()
                        .content(format!("-# using {}...", tool_name))
                        .flags(MessageFlags::SUPPRESS_EMBEDS);
    
                    if let Err(e) = msg.channel_id.send_message(&ctx.http, status_res).await {
                        debug!("Error sending message: {:?}", e);
                    }
                }
            }
            
            // 推論の上限回数を超えた場合はツールモードを無効化
            let mode = if i == *MAX_USE_TOOL_COUNT {
                ToolMode::Disable
            } else {
                ToolMode::Auto
            };
            // 推論の実行
            match reasoning_stream.proceed(&mode).await {
                Err(e) => {
                                return format!("Err: failed reasoning - {:?}", e);
                            }
                Ok(_) => {},
            }
        }

        // 推論結果の取得
        let content = reasoning_stream.content.unwrap_or("Err: response is none from ai".to_string());

        // ツールコールの統計収集
        let model_info = format!("\n-# model: {}", prompt_stream.client.model_config.unwrap().model);
        let mut tool_count = HashMap::new();
        for tool in used_tools {
            *tool_count.entry(tool).or_insert(0) += 1;
        }
        let used_tools_info = if !tool_count.is_empty() {
            let tools_info: Vec<String> = tool_count.iter().map(|(tool, count)| {
                if *count > 1 {
                    format!("{} x{}", tool, count)
                } else {
                    tool.clone()
                }
            }).collect();
            format!("\n-# tools: {}", tools_info.join(", "))
        } else {
            "".to_string()
        };
        // プロンプトストリームに分岐した分部をマージ
        let differential_stream = prompt_stream.prompt.split_off(last_pos + 1 /* 先頭のシステムプロンプト消す */);
        {
            let mut r_prompt_stream = self.prompt_stream.lock().await;
            r_prompt_stream.add(differential_stream.into()).await;
        }
        return content.replace("\\n", "\n") + &model_info + &used_tools_info;
    }

    pub async fn add_message(&self, mut message: InputMessage) {
        let user_prompt = ChannelState::prepare_user_prompt(&mut message, 1).await;
        let mut prompt_stream = self.prompt_stream.lock().await;



        prompt_stream.add(user_prompt).await;
    }

    pub async fn clear_prompt(&self) {
        let mut prompt_stream = self.prompt_stream.lock().await;
        prompt_stream.clear().await;
    }
}