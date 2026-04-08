use std::time::Instant;

use log::{error, info};
use poise::CreateReply;
use serenity::all::{CreateAttachment, User, UserId};

use crate::{config::Models, context::ObserverContext, tools::latex::LatexExprRenderTool};

// エラー型（とりあえず Box に投げるスタイルでOK）
type Error = Box<dyn std::error::Error + Send + Sync>;

// 毎回書くのがだるいので type alias
type Context<'a> = poise::Context<'a, ObserverContext, Error>;

/// ping pong..
#[poise::command(slash_command, prefix_command)]
pub async fn ping(
    ctx: Context<'_>
) -> Result<(), Error> {
    let start = Instant::now();

    // まずメッセージ送信
    let msg = ctx.say("Pinging...").await?;

    let elapsed = start.elapsed().as_millis();

    // CreateReply を作って渡す
    msg.edit(
        ctx,
        CreateReply::default().content(format!("Pong! `{elapsed}ms`")),
    )
    .await?;

    Ok(())
}

/// only admin user
#[poise::command(slash_command, prefix_command)]
pub async fn set_system_prompt(
    ctx: Context<'_>,

    #[description = "System prompt to set (or 'reset' to default)"]
    system_prompt: String,
) -> Result<(), Error> {
    let ob_ctx = ctx.data();

    let caller_id_u64 = ctx.author().id.get();
    if !ob_ctx.config.admin_users.contains(&caller_id_u64) {
        ctx.say("Err: you are not allowed to use /set_system_prompt.").await?;
        return Ok(());
    }

    let channel_id = ctx.channel_id();

    if system_prompt.eq_ignore_ascii_case("reset") {
        ob_ctx.chat_contexts.set_system_prompt(channel_id, None);
        ctx.say("info: System prompt reset to default.").await?;
    } else {
        ob_ctx.chat_contexts.set_system_prompt(channel_id, Some(system_prompt.clone()));
        ctx.say(format!("info: System prompt set to:\n```{}```", system_prompt)).await?;
    }

    Ok(())
}

/// only admin user
#[poise::command(slash_command, prefix_command)]
pub async fn rate_config(
    ctx: Context<'_>,

    #[description = "Target user"]
    target_user: User,  // ← ここが Discord のユーザー選択になる

    #[description = "consumption cost value: 'unlimit' or a number"]
    #[autocomplete = "autocomplete_rate_limit"]
    limit: String,
) -> Result<(), Error> {
    let ob_ctx = ctx.data();

    let caller_id_u64 = ctx.author().id.get();
    if !ob_ctx.config.admin_users.contains(&caller_id_u64) {
        ctx.say("Err: you are not allowed to use /rate_config.").await?;
        return Ok(());
    }

    let target_user_id: UserId = target_user.id;

    let new_rate_line: u64 = if limit.eq_ignore_ascii_case("unlimit") {
        0
    } else if limit.eq_ignore_ascii_case("reset") {
        1
    } else {
        let cost = match limit.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                ctx.say("Err: limit must be 'unlimit' or a number.").await?;
                return Ok(());
            }
        };
        ob_ctx.user_contexts.get_or_create(target_user_id).rate_line + cost * ob_ctx.config.rate_limit_sec_per_cost
    };

    ob_ctx.user_contexts.set_rate_line(target_user_id, new_rate_line);

    let reply = if new_rate_line == 0 {
        format!(
            "info: Set rate-line for user `{}` to **unlimit**.",
            target_user_id
                .to_user(ctx.http())
                .await
                .map(|u| u.display_name().to_string())
                .unwrap_or_else(|_| "Null".to_string())
        )
    } else {
        format!(
            "info: Set rate-line for user `{}` to **{}**.",
            target_user_id.to_user(ctx.http()).await.map(|u| u.display_name().to_string()).unwrap_or_else(|_| "Null".to_string()),
            new_rate_line
        )
    };

    ctx.say(reply).await?;
    Ok(())
}


/// `/rate_config` の第2引数 `limit` 用のオートコンプリート
async fn autocomplete_rate_limit(
    _ctx: Context<'_>,
    partial: &str,
) -> Vec<String> {
    let base_candidates = [
        "unlimit",
        "reset",
        "1",
        "2",
        "3",
        "5",
        "10",
        "30",
        "60",
        "120",
        "300",
        "600",
        "1800",
        "3600",
    ];

    let p = partial.to_lowercase();

    let mut out: Vec<String> = base_candidates
        .iter()
        .filter(|v| v.to_lowercase().starts_with(&p))
        .map(|v| v.to_string())
        .collect();

    out.sort();
    out.dedup();
    out.truncate(20);
    out
}

/// clear context
#[poise::command(slash_command, prefix_command)]
pub async fn clear(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id();

    let ob_ctx = ctx.data();

    ob_ctx.chat_contexts.clear(channel_id);

    info!("Cleared chat context for channel {}", channel_id);

    ctx.say("info: Cleared chat context.").await?;

    Ok(())
}

/// to enable observer bot
#[poise::command(slash_command, prefix_command)]
pub async fn enable(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id();

    let ob_ctx = ctx.data();

    if ob_ctx.chat_contexts.is_enabled(channel_id) {
        ctx.say("info: Chat context is already enabled in this channel.").await?;
        return Ok(());
    } else {
        ob_ctx.chat_contexts.set_enabled(channel_id, true);
        ob_ctx.chat_contexts.get_or_create(channel_id);
        ctx.say("info: Chat context enabled in this channel.").await?;
        info!("Enabled chat context for channel {}", channel_id);
        return Ok(());
    }
}

/// to disable observer bot
#[poise::command(slash_command, prefix_command)]
pub async fn disable(ctx: Context<'_>) -> Result<(), Error> {
    let channel_id = ctx.channel_id();

    let ob_ctx = ctx.data();

    if !ob_ctx.chat_contexts.is_enabled(channel_id) {
        ctx.say("info: Chat context is already disabled in this channel.").await?;
        return Ok(());
    } else {
        ob_ctx.chat_contexts.set_enabled(channel_id, false);
        ctx.say("info: Chat context disabled in this channel.").await?;
        info!("Disabled chat context for channel {}", channel_id);
        return Ok(());
    }
}

/// model config command
#[poise::command(slash_command, prefix_command, subcommands("get", "set", "list"))]
pub async fn model(_: Context<'_>) -> Result<(), Error> {
    Ok(()) // ここはメインでは使わない
}

#[poise::command(slash_command, prefix_command)]
pub async fn get(ctx: Context<'_>) -> Result<(), Error> {
    let ob_ctx = ctx.data();
    let user_id = ctx.author().id;
    let model = ob_ctx.user_contexts.get_or_create(user_id).main_model.clone();
    ctx.say(format!("Current model: `{}`", model)).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let models = Models::list();

    let mut s = String::from("**List of models:**\n");
    for m in models {
        s.push_str(&format!("- `{}`\n", m));
    }

    ctx.say(s).await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn set(
    ctx: Context<'_>,
    #[description = "Choose a model"]
    #[autocomplete = "autocomplete_model_name"]
    model_name: String,
) -> Result<(), Error> {
    let ob_ctx = ctx.data();
    let user_id = ctx.author().id;
    let model = Models::from(model_name);
    ob_ctx.user_contexts.set_model(user_id, model.clone());

    ctx.say(format!("info: Changed model to `{}`", model)).await?;
    Ok(())
}

async fn autocomplete_model_name(
    _ctx: Context<'_>,
    partial: &str,
) -> Vec<String> {
    let models = Models::list();
    models
        .into_iter()
        .filter(|m| m.to_string().starts_with(partial))
        .map(|m| m.to_string())
        .collect()
}

/// latex expr render
#[poise::command(slash_command, prefix_command)]
pub async fn tex_expr(
    ctx: Context<'_>,
    #[description = "LaTeX expression to render"]
    #[autocomplete = "autocomplete_tex_expr"]
    expr: String,
) -> Result<(), Error> {
    let ob_ctx = ctx.data();

    // レンダリング実行（ヘッドレスブラウザ経由）
    let png_bytes = match LatexExprRenderTool::render(&expr, ob_ctx).await {
        Ok(bytes) => bytes,
        Err(e) => {
            error!("Failed to render LaTeX expression `{}`: {}", expr, e);
            ctx.say(format!("error: Failed to render LaTeX expression: {}", e))
            .await?;
            return Ok(());
        }
    };

    let attachment = CreateAttachment::bytes(png_bytes, "tex_expr.png");

    ctx.send(
        CreateReply::default()
            .attachment(attachment)
    )
    .await?;

    Ok(())
}

async fn autocomplete_tex_expr(
    _ctx: Context<'_>,
    partial: &str,
) -> Vec<String> {
    // LaTeX コマンド単体候補
    const COMMANDS: &[&str] = &[
        r"\alpha", r"\beta", r"\gamma", r"\delta",
        r"\sin", r"\cos", r"\tan",
        r"\log", r"\ln",
        r"\sqrt{}", r"\frac{}{}",
        r"\int_0^1", r"\sum_{n=0}^{\infty}", r"\prod_{i=1}^{n}",
        r"\lim_{x \to 0}", r"\infty",
        r"\mathbb{R}", r"\mathbb{Z}", r"\mathbb{N}",
    ];

    // ある程度完成された数式テンプレ
    const SNIPPETS: &[&str] = &[
        r"\int_0^1 x^2 \, dx",
        r"\sum_{n=0}^{\infty} a_n x^n",
        r"\lim_{x \to 0} \frac{\sin x}{x}",
        r"e^{i\pi} + 1 = 0",
        r"a^2 + b^2 = c^2",
        r"\frac{d}{dx} f(x)",
        r"\nabla \cdot \vec{E} = \frac{\rho}{\varepsilon_0}",
    ];

    let mut candidates: Vec<String> = Vec::new();

    // まずコマンド候補
    for &c in COMMANDS {
        if partial.is_empty()
            || c.starts_with(partial)
            || c.contains(partial)
        {
            candidates.push(c.to_string());
        }
    }

    // つぎにテンプレ数式
    for &s in SNIPPETS {
        if partial.is_empty()
            || s.starts_with(partial)
            || s.contains(partial)
        {
            candidates.push(s.to_string());
        }
    }

    // ダブり削除 & 最大 20 個くらいに絞る
    candidates.sort();
    candidates.dedup();
    candidates.truncate(20);

    candidates
}


pub fn log_err(context: &str, err: &(dyn std::error::Error + Send + Sync)) {
    error!("[{context}] {err:#?}");

    let mut src = err.source();
    while let Some(s) = src {
        error!("  caused by: {s:?}");
        src = s.source();
    }
    error!("error trace end");
}
