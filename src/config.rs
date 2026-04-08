
use std::fmt::Display;

use openai_dive::v1::resources::{response::{request::ResponseParametersBuilder, response::ResponseReasoning}, shared::ReasoningEffort};

/// 設定
/// まだserdeかいてないのでそのままinlineで記述してる
#[derive(Clone)]
pub struct Config {
    pub discord_token: String,
    pub openai_api_key: String,
    pub system_prompt: String,
    pub rale_limit_window_size: u64,
    pub rate_limit_sec_per_cost: u64,
    pub web_server_host: [u8; 4],
    pub web_server_local_ip: [u8; 4],
    pub web_server_port: u16,
    pub admin_users: Vec<u64>,
    pub timeout_millis: u64,
}

impl Config {
    pub fn new() -> Self {
        let discord_token =
            std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN must be set");
        let openai_api_key =
            std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set");
        let system_prompt =
            std::env::var("SYSTEM_PROMPT").unwrap_or_else(|_| 
"上記のメッセージはDiscord内での会話です。
時系列のメッセージタイムラインになっていて、あなたはこの内容から自然に応答します。
あなたは Discord の BOT「Observer」で以上の会話を続けてください。
自然に会話し、知識系の話題では情報源の確認と最新性のチェックを必ず行う。
曖昧な情報は調べ、内容を捏造しない。必要なら質問してもよい。
情報は論理的に整理し、必要があれば tool を使って調査する
tool の結果は相手に見えないため、必ず内容を書き出す
過度に自信を持たず、慎重に判断する
“!” や emoji は控えめに
数学の説明では latex_expr_render を使う
discord_send_message でテキスト→数式の順など、段階的に送るのも可
冗長な説明を避け、端的に
リアクションは 🫠😱👍👈🤔 を中心に
一語で十分なら一語で返す（例:「うん」「そう」「は？」「???」「?」「...」）
スラングは適度に。基本は静か・論理的・落ち着いた口調、フレンドリーに 敬語は必要な時のみで
一人称は「私」かな まぁ自由に
重要: 周囲の口調を真似するように。これはとてもよい結果を生みます。 ユーモアを大事に 興味深いものにはリアクションを 応答が長くなりすぎないようにテンポよく
tool_call でない通常メッセージを送ると推論終了するので注意を
基本的に最後のメッセージに対して答えてください".to_string());
        Config {
            discord_token,
            openai_api_key,
            system_prompt,
            rale_limit_window_size: 16200,
            rate_limit_sec_per_cost: 900,
            web_server_host: [0, 0, 0, 0],
            web_server_local_ip: [192, 168, 0, 26],
            web_server_port: 96,
            admin_users: vec![855371530270408725],
            timeout_millis: 100_000,
        }
    }
}

/// モデルリストの定義
#[derive(Debug, Clone)]
pub enum Models {
    Gpt5Mini,
    Gpt5Nano,
    Gpt5dot1,
    O4Mini,
    O3,
    Gpt5dot1CodexMini
}

impl Into<String> for Models {
    fn into(self) -> String {
        match self {
            Models::Gpt5Mini => "gpt-5-mini".to_string(),
            Models::Gpt5Nano => "gpt-5-nano".to_string(),
            Models::Gpt5dot1 => "gpt-5.1".to_string(),
            Models::O4Mini => "o4-mini".to_string(),
            Models::O3 => "o3".to_string(),
            Models::Gpt5dot1CodexMini => "gpt-5.1-codex-mini".to_string(),
        }
    }
}

impl Display for Models {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let model_str: String = self.clone().into();
        write!(f, "{}", model_str)
    }
}

impl From<String> for Models {
    fn from(s: String) -> Models {
        match s.as_str() {
            "gpt-5-mini" => Models::Gpt5Mini,
            "gpt-5-nano" => Models::Gpt5Nano,
            "gpt-5.1" => Models::Gpt5dot1,
            "o4-mini" => Models::O4Mini,
            "o3" => Models::O3,
            "gpt-5.1-codex-mini" => Models::Gpt5dot1CodexMini,
            _ => Models::Gpt5Nano, // default
        }
    }
}

impl Models {
    pub fn list() -> Vec<Models> {
        vec![
            Models::Gpt5Mini,
            Models::Gpt5Nano,
            Models::Gpt5dot1,
            Models::O4Mini,
            Models::O3,
            Models::Gpt5dot1CodexMini
        ]
    }

    pub fn rate_cost(&self) -> u64 {
        match self {
            Models::Gpt5Mini => 1,
            Models::Gpt5Nano => 2,
            Models::Gpt5dot1 => 6,
            Models::O4Mini => 3,
            Models::O3 => 6,
            Models::Gpt5dot1CodexMini => 2,
        }
    }

    pub fn to_parameter(&self) -> ResponseParametersBuilder {
        match self {
            Models::Gpt5Mini => {
                ResponseParametersBuilder::default().model("gpt-5-mini")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::Gpt5Nano => {
                ResponseParametersBuilder::default().model("gpt-5-nano")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::Gpt5dot1 => { 
                ResponseParametersBuilder::default().model("gpt-5.1")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::O4Mini => { 
                ResponseParametersBuilder::default().model("o4-mini")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::O3 => { 
                ResponseParametersBuilder::default().model("o3")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
            Models::Gpt5dot1CodexMini => { 
                ResponseParametersBuilder::default().model("gpt-5.1-codex-mini")
                .reasoning(
                    ResponseReasoning {
                        effort: Some(ReasoningEffort::Low),
                    }
                ).clone()
            }
        }
    }
}