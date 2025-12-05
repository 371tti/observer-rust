use std::{error::Error, time::{Duration, Instant}};

use log::{debug, info};
use openai_dive::v1::resources::response::{request::{ContentInput, ContentItem, ImageDetailLevel, InputMessage}, response::Role};
use serenity::all::{ActivityData, CreateMessage, EditMessage, FullEvent, Message};
use tokio::{sync::mpsc, time::sleep};


use crate::{commands::log_err, context::ObserverContext, lmclient::LMContext};


/// イベントハンドラ
/// serenity poise へ渡すもの
pub async fn event_handler(
    ctx: &serenity::client::Context,
    event: &FullEvent,
    framework: poise::FrameworkContext<'_, ObserverContext, Box<dyn Error + Send + Sync>>,
    data: &ObserverContext,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match event {
        // メッセージうけとったとき
        FullEvent::Message { new_message } => {
            handle_message(ctx, new_message, framework, data).await?;
        }
        // 初期化完了
        FullEvent::Ready { data_about_bot } => {
            info!("Bot is connected as {}", data_about_bot.user.name);
            update_presence(ctx).await;
        }
        // あたらしいギルドに参加
        FullEvent::GuildCreate { guild, is_new: _ } => {
            info!("Joined new guild: {} (id: {})", guild.name, guild.id);
            update_presence(ctx).await;
        }
        // リアクション通知
        FullEvent::ReactionAdd { add_reaction } => {
            debug!("Reaction added: {:?} by user {:?}", add_reaction.emoji, add_reaction.user_id);
            handle_emoji_reaction(add_reaction, data).await?;
        }

        _ => { /* 他のイベントは無視 */ }
    }

    Ok(())
}

/// ステータスメッセージの更新
async fn update_presence(ctx: &serenity::client::Context) {
    let guild_count = ctx.cache.guilds().len();

    ctx.set_activity(Some(ActivityData::playing(
        format!("in {} servers", guild_count)
    )));
}

async fn handle_emoji_reaction(
    reaction: &serenity::model::channel::Reaction,
    ob_context: &ObserverContext
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // ここでリアクション追加時の処理を実装可能
    let channel_id = reaction.channel_id;
    let message_id = reaction.message_id;
    let member = reaction.member.clone().unwrap_or_default();
    let user_id = member.user.name.clone();
    let user_display_name = member.user.display_name().to_string();
    let mut lm_context = LMContext::new();
    lm_context.add_text(
        serde_json::json!({
            "user": user_id,
            "display_name": user_display_name,
            "added_reaction": format!("{:?}", reaction.emoji),
            "message_id": message_id.to_string(),
            "channel_id": channel_id.to_string()
        }).to_string(),
        Role::User,
    );
    ob_context.chat_contexts.marge(channel_id, &lm_context);

    debug!("Handling emoji reaction: {:?} by user {:?}", reaction.emoji, reaction.user_id);
    Ok(())
}


/// メッセージを受け取ったときの処理
async fn handle_message(
    ctx: &serenity::client::Context,
    msg: &Message,
    _framework: poise::FrameworkContext<'_, ObserverContext, Box<dyn Error + Send + Sync>>,
    ob_context: &ObserverContext,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let start = Instant::now();
    let channel_id = msg.channel_id;

    let bot_id = ctx.cache.current_user().id;
    // 自分のメッセージは無視
    if msg.author.id == bot_id {
        return Ok(());
    }

    let is_mentioned = msg.mentions_user_id(bot_id);

    let content = serde_json::json!({
        "user": msg.author.name,
        "display_name": msg.author.display_name(),
        "msg_id": msg.id.to_string(),
        "reply_to": msg.referenced_message.as_ref().map_or("None".to_string(), |m| m.id.to_string()),
        "content": msg.content
    }).to_string();

    // 添付画像のURLを取る
    let image_urls: Vec<String> = msg
        .attachments
        .iter()
        .filter(|att| {
            // content_type が "image/..." なら画像とみなす
            if let Some(ct) = &att.content_type {
                ct.starts_with("image/")
            } else {
                // 拡張子で雑に判定する fallback
                att.filename.ends_with(".png")
                    || att.filename.ends_with(".jpg")
                    || att.filename.ends_with(".jpeg")
                    || att.filename.ends_with(".webp")
            }
        })
        .map(|att| att.url.clone())
        .collect();

    let mut lm_context = LMContext::new();

    if image_urls.is_empty() && content.is_empty() {
        // 画像もテキストも無いなら無視
        return Ok(());
    } else if image_urls.is_empty() {
        debug!("Adding text message to context in channel {}, content: {}", channel_id, content);
        lm_context.add_text(content.clone(), Role::User);
    } else {
        debug!("Adding image message to context in channel {}, content: {}", channel_id, content);
        lm_context.add_message(InputMessage {
            role: Role::User,
            content: ContentInput::List(
                {
                    let mut items = Vec::new();
                    items.push(ContentItem::Text {
                        text: content.clone(),
                    });
                    for url in image_urls.iter() {
                        items.push(ContentItem::Image {
                            detail: ImageDetailLevel::Low,
                            file_id: None,
                            image_url: Some(url.clone()),
                        });
                    }
                    items
                }
            )
        });
    }

    ob_context.chat_contexts.marge(channel_id, &lm_context);

    if is_mentioned {
        if !ob_context.chat_contexts.is_enabled(channel_id) {
            msg.channel_id
                .send_message(&ctx.http, CreateMessage::new().content("info: Chat context is disabled in this channel."))
                .await?;
            return Ok(());
        }
        let user_id = msg.author.id;
        let user_ctx = ob_context.user_contexts.get_or_create(user_id);
        let model = user_ctx.main_model.clone();

        let model_cost = model.rate_cost();
        let sec_per_cost = ob_context.config.rate_limit_sec_per_cost; // コストあたりの秒数
        let window_size = ob_context.config.rale_limit_window_size; // バースト許容量
        let user_line = user_ctx.rate_line;
        
        // レートリミットの計算
        let time_stamp = chrono::Utc::now().timestamp() as u64;
        let limit_line = window_size + time_stamp;
        let add_line = model_cost * sec_per_cost;
        let added_user_line = if user_line == 0 {
            0 // リミットレスアカウント
        } else if user_line < time_stamp {
            time_stamp + add_line
        } else {
            user_line + add_line
        };

        if added_user_line > limit_line {
            let wait_sec = added_user_line - limit_line;
            let allow_ts = time_stamp + wait_sec;
            msg.channel_id
                .send_message(&ctx.http, CreateMessage::new().content(format!("Err: rate limit - try again after <t:{}:R>", allow_ts)))
                .await?;
            return Ok(());
        }
        ob_context.user_contexts.set_rate_line(user_id, added_user_line);

        let typing_ctx = ctx.clone();

        let typing_handle = tokio::spawn(async move {
            loop {
                let _ = channel_id.broadcast_typing(&typing_ctx.http).await;
                sleep(Duration::from_secs(5)).await; // だいたい5秒おきでOK
            }
        });
        let mut context = ob_context.chat_contexts.get_or_create(channel_id);
        let tools = ob_context.tools.clone();

        let system_prompt = format!{
            "{}\n current channel_id: {}, channel_name: {}",
            ob_context.chat_contexts.get_system_prompt(channel_id),
            msg.channel_id, 
            msg.channel_id.name(&ctx.http).await.unwrap_or("None".to_string()),
        };

        context.add_message(InputMessage {
            role: Role::System,
            content: ContentInput::Text(system_prompt),
        });

        let mut thinking_msg = msg
            .channel_id
            .send_message(
                &ctx.http,
                CreateMessage::new().content("-# Thinking..."),
            )
            .await?;

        // streaming 用チャネル
        let (state_tx, mut state_rx) = mpsc::channel::<String>(100);
        let (delta_tx, mut delta_rx) = mpsc::channel::<String>(100);

        let mut result = None;

        let timeout_duration = Duration::from_millis(ob_context.config.timeout_millis);

            
        tokio::select! {
            biased;

            r = ob_context.lm_client.generate_response(ob_context.clone(), &context, Some(2000), Some(tools), Some(state_tx), Some(delta_tx), Some(model.to_parameter())) => {
                if let Err(e) = &r {
                    log_err("Error generating response", e.as_ref());
                    thinking_msg
                        .edit(&ctx.http, EditMessage::new().content("-# Error during reasoning"))
                        .await
                        .ok();
                    typing_handle.abort();
                }
                result = Some(r?); // ?でエラー処理も可能
                debug!("Response generation completed");
                
            }
            _ = async {
                let mut last_edit = Instant::now() + Duration::from_millis(550);      // 前回 edit した時間
                let mut swap = String::new();          // 状態保存用バッファ

                while let Some(state) = state_rx.recv().await {
                    swap = state;

                    if last_edit.elapsed() < Duration::from_millis(550) {
                        continue;
                    }

                    // 1秒経過 → 最新 state だけ使って edit
                    thinking_msg
                        .edit(&ctx.http, EditMessage::new().content(format!("-# {}", swap)))
                        .await
                        .ok();
                    last_edit = Instant::now(); // 時刻更新
                }
            } => {}
            _ = async {
                while let Some(delta) = delta_rx.recv().await {
                    info!("Delta received: {}", delta);
                }
            } => {}
            _ = sleep(timeout_duration) => {
                thinking_msg
                    .edit(&ctx.http, EditMessage::new().content("-# Error timeout"))
                    .await
                    .ok();
                typing_handle.abort();
                return Ok(());
            }
        }
    
        let result = result.unwrap();

        // 返ってきた結果をコンテキストにマージ
        ob_context.chat_contexts.marge(channel_id, &result);

        let elapsed = start.elapsed().as_millis();
        let text = result.get_result();

        debug!("Final response: {}ms \"{}\"", elapsed, text);

        // タイピング通知停止
        typing_handle.abort();

        let model = ob_context.user_contexts.get_or_create(user_id).main_model;

        // 「Thinking...」を削除して最終回答を表示
        thinking_msg.delete(&ctx.http).await.ok();

        msg.channel_id
            .send_message(&ctx.http, CreateMessage::new().content(format!("{}\n-# Reasoning done in {}ms, model: {}", text, elapsed, model)))
            .await?;
    }


    Ok(())
}
