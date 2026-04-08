use std::{collections::{HashMap, VecDeque}, sync::Arc};

use log::{debug, error, info, warn};
use openai_dive::v1::{api::Client, resources::response::{items::{FunctionToolCall, FunctionToolCallOutput, ReasoningSummaryPart}, request::{ContentInput, ContentItem, ImageDetailLevel, InputItem, InputMessage, ResponseInput, ResponseInputItem, ResponseParametersBuilder}, response::{OutputContent, ResponseOutput, ResponseStreamEvent, Role}, shared::{ResponseTool, ResponseToolChoice}}};
use serenity::futures::{StreamExt};
use tokio::sync::mpsc;

use crate::{config::Models, context::ObserverContext};
pub struct LMClient {
    pub client: Client,
}


/// LMのクライアント
/// レスポンス投げて返すための抽象レイヤ
impl LMClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn generate_response(
        &self,
        ob_ctx: ObserverContext,
        lm_context: &LMContext,
        max_tokens: Option<u32>,
        tools: Option<Arc<HashMap<String, Box<dyn LMTool>>>>,
        state_mpsc: Option<mpsc::Sender<String>>,
        delta_mpsc: Option<mpsc::Sender<String>>,
        parameters: Option<ResponseParametersBuilder>,
    ) -> Result<LMContext, Box<dyn std::error::Error + Send + Sync>> {

        debug!("Generating response with context: {:?}", lm_context);
        let tools = tools.unwrap_or_default();
        let state_send = |s: String| {
            if let Some(tx) = state_mpsc.as_ref() {
                let _ = tx.clone().try_send(s);
            }
        };

        let delta_send = |s: String| {
            if let Some(tx) = delta_mpsc.as_ref() {
                let _ = tx.clone().try_send(s);
            }
        };
        let tool_defs = tools.iter().map(|(_name, tool)| tool.define()).collect::<Vec<ResponseTool>>();


        let per_parameters = parameters.unwrap_or_else(|| {
            ResponseParametersBuilder::default()
                .model(Models::Gpt5Nano).clone()
        })
            .max_output_tokens(max_tokens.unwrap_or(100))
            .parallel_tool_calls(true)
            .tools(tool_defs).clone();

        let mut tool_choice = ResponseToolChoice::Auto;

        let mut delta_context = LMContext::new();

        let mut token_count = 0;
        
        for i in 0..10 {
            let context = lm_context.generate_context_with(&delta_context);
            debug!("Iteration {}: Generated context: {:?}", i, context);
            let parameters = per_parameters.clone()
                .input(context)
                .tool_choice(tool_choice.clone())
                .build()
                .unwrap();

            let mut result = self.client.responses().create_stream(parameters).await?;

            while let Some(chunk) = result.next().await {
                let chunk = chunk.map_err(|e| {
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                })?;
                match chunk {
                    ResponseStreamEvent::ResponseCreated { sequence_number, response: _ } => {
                        state_send(format!("Response created (seq {})", sequence_number));
                        info!("Response created (seq {})", sequence_number);
                    },
                    ResponseStreamEvent::ResponseQueued { sequence_number, response: _ } => {
                        state_send(format!("Response queued... (seq {})", sequence_number));
                        info!("Response queued (seq {})", sequence_number);
                    },
                    ResponseStreamEvent::ResponseInProgress { sequence_number, response: _ } => {
                        state_send(format!("Response in progress... (seq {})", sequence_number));
                        info!("Response in progress (seq {})", sequence_number);
                    },
                    ResponseStreamEvent::ResponseCompleted { sequence_number, response: _ } => {
                        info!("Response completed (seq {})", sequence_number);
                        break;
                    },

                    ResponseStreamEvent::ResponseFailed { sequence_number, response } => {
                        error!("Response failed (seq {}): {:?}", sequence_number, response);
                        return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Response failed")));
                    },
                    ResponseStreamEvent::ResponseIncomplete { sequence_number, response } => {
                        error!("Response incomplete (seq {}): {:?}", sequence_number, response);
                        return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, "Response incomplete")));
                    },

                    ResponseStreamEvent::ResponseOutputItemDone { sequence_number: _, output_index: _, item } => {
                        match item {
                            ResponseOutput::Message(output_message) => {
                                delta_context.add_text(
                                    output_message.content.iter().map(|r| match r {
                                        OutputContent::Text { text, annotations: _ } => text.clone(),
                                        _ => "".to_string(),
                                    }).collect::<Vec<String>>().join(""),
                                    Role::Assistant,
                                );
                            },
                            ResponseOutput::FunctionToolCall(function_tool_call) => {
                            state_send(format!("Function tool call: {}", function_tool_call.name));
                                delta_context.add_input_item(InputItem::FunctionToolCall(
                                    function_tool_call.into()
                                ));
                            },
                            ResponseOutput::FileSearchToolCall(file_search_tool_call) => {
                                delta_context.add_input_item(InputItem::FileSearchToolCall(
                                    file_search_tool_call.into()
                                ));
                            },
                            ResponseOutput::WebSearchToolCall(web_search_tool_call) => {
                                delta_context.add_input_item(InputItem::WebSearchToolCall(
                                    web_search_tool_call.into()
                                ));
                            },
                            ResponseOutput::ComputerToolCall(computer_tool_call) => {
                                delta_context.add_input_item(InputItem::ComputerToolCall(
                                    computer_tool_call.into()
                                ));
                            },
                            ResponseOutput::Reasoning(reasoning) => {
                                delta_context.add_input_item(InputItem::Reasoning(
                                    reasoning.into()
                                ));
                            },
                            _ => {
                                warn!("Unhandled output item: {:?}", item);
                            }
                        }
                    },

                    ResponseStreamEvent::ResponseOutputTextDelta { sequence_number: _, item_id: _, output_index: _, content_index: _, delta, logprobs: _ } => {
                        delta_send(delta);
                        token_count += 1;
                        state_send(format!("Generating... ({} tokens)", token_count));
                    },

                    ResponseStreamEvent::ResponseRefusalDone { sequence_number: _, item_id: _, output_index: _, content_index: _, refusal } => {
                        state_send(refusal);
                    },

                    ResponseStreamEvent::ResponseReasoningSummaryPartDone { sequence_number: _, item_id: _, output_index: _, summary_index: _, part } => {
                        state_send(match part {
                            ReasoningSummaryPart::SummaryText { text } => text,
                        });
                    },

                    ResponseStreamEvent::Error { sequence_number, code, message, param } => {
                        error!("Error (seq {}): {} - {} ({:?})", sequence_number, code, message, param);
                        return Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, message)));
                    },
                    _ => {
                        warn!("Unhandled stream event: {:?}", chunk);
                    }
                }
            }

            let mut outputs = Vec::new();
            let uncompleted_tool_calls = delta_context.get_uncompleted_tool_calls();
            if uncompleted_tool_calls.is_empty() {
                break;
            }
            for tool_call in uncompleted_tool_calls {
                debug!("Executing tool call: {:?}", tool_call);
                let name = tool_call.name.clone();
                let args = tool_call.arguments.clone();
                let c_id: String = tool_call.call_id.clone();

                let v_args: serde_json::Value = serde_json::from_str(&args)
                    .unwrap_or(serde_json::Value::Null);
                // $explainがあればとってくる
                let explain = v_args.as_object().and_then(|o| 
                    o.get("properties").and_then(|o|
                    o.as_object().and_then(|o| 
                    o.get("$explain").and_then(|o|
                    o.as_str()
                    ))));
                if let Some(explain) = explain {
                    state_send(format!("Executing tool: {} - {}", name, explain));
                } else {
                    state_send(format!("Executing tool: {}", name));
                }

                // ここでtoolを実行
                if let Some(tool) = tools.get(&name) {
                    let exec_result = tool.execute(v_args, ob_ctx.clone()).await;
                    debug!("Tool {} executed with result: {:?}", name, exec_result);
                    let output = match exec_result {
                        Ok(res) => FunctionToolCallOutput {
                            call_id: c_id.clone(),
                            output: res,
                            id: None,
                        },
                        Err(err) => FunctionToolCallOutput {
                            call_id: c_id.clone(),
                            output: format!("Error: {}", err),
                            id: None,
                        },
                    };
                    outputs.push(output);
                }
            }

            for output in outputs {
                delta_context.add_input_item(InputItem::FunctionToolCallOutput(output));
            }



            if i == 8 {
                tool_choice = ResponseToolChoice::None;
            }
        }



        Ok(delta_context)
    }
}

/// コンテキスト実態
/// リングバッファで管理
#[derive(Debug, Clone)]
pub struct LMContext {
    pub buf: VecDeque<ResponseInputItem>,
    pub max_len: usize,
}

impl LMContext {
    pub fn new() -> Self {
        Self {
            buf: VecDeque::new(),
            max_len: 64,
        }
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }

    pub fn set_max_len(&mut self, max_len: usize) {
        self.max_len = max_len;
    }

    pub fn generate_context(&self) -> ResponseInput {
        ResponseInput::List(self.buf.clone().into())
    }

    pub fn generate_context_with(&self, additional: &LMContext) -> ResponseInput {
        let mut combined = self.buf.clone();
        for item in additional.buf.iter() {
            combined.push_back(item.clone());
        }
        ResponseInput::List(combined.into())
    }

    pub fn extend(&mut self, other: &LMContext) {
        for item in other.buf.iter() {
            if let ResponseInputItem::Item(_) = item {
                continue;
            }
            self.buf.push_back(item.clone());
        }
        self.trim_len();
    }

    pub fn trim_len(&mut self) {
        while self.buf.len() > self.max_len {
            self.buf.pop_front();
        }
    }

    pub fn add_text(&mut self, text: String, role: Role) {
        self.buf.push_back(ResponseInputItem::Message(
            InputMessage {
                role,
                content: ContentInput::Text(text)
            }
        ));
    }

    pub fn add_text_with_image(&mut self, text: String, image_url: String, role: Role, detail: ImageDetailLevel) {
        self.buf.push_back(ResponseInputItem::Message(
            InputMessage {
                role,
                content: ContentInput::List(vec![
                    ContentItem::Text { text },
                    ContentItem::Image {
                        detail: detail,
                        file_id: None,
                        image_url: Some(image_url),
                    }
                ])
            }
        ));
    }

    pub fn add_message(&mut self, message: InputMessage) {
        self.buf.push_back(ResponseInputItem::Message(message));
    }

    pub fn add_input_item(&mut self, item: InputItem) {
        self.buf.push_back(ResponseInputItem::Item(item));
    }

    pub fn get_latest(&self) -> Option<&ResponseInputItem> {
        self.buf.back()
    }

    pub fn get_result(&self) -> String {
        let mut result = String::new();
        let latest = self.get_latest();
        if let Some(ResponseInputItem::Message(msg)) = latest {
            match &msg.content {
                ContentInput::Text(text) => {
                    result.push_str(text);
                },
                ContentInput::List(items) => {
                    for item in items {
                        match item {
                            ContentItem::Text { text } => {
                                result.push_str(text);
                            },
                            _ => {}
                        }
                    }
                }
            }
        }
        result
    }

    pub fn get_uncompleted_tool_calls(&mut self) -> Vec<&FunctionToolCall> {
        // 同じcall_idが存在しないInputItemを集める
        let call_id_list = self.buf.iter().filter_map(|item| {
            if let ResponseInputItem::Item(InputItem::FunctionToolCallOutput(call)) = item {
                Some(call.call_id.clone())
            } else {
                None
            }
        }).collect::<Vec<String>>();

        self.buf.iter().filter_map(|item| {
            if let ResponseInputItem::Item(InputItem::FunctionToolCall(call)) = item {
                if !call_id_list.contains(&call.call_id) {
                    return Some(call);
                }
            }
            None
        }).collect()
    }
}

#[async_trait::async_trait]
pub trait LMTool: Send + Sync {
    fn define(&self) -> ResponseTool {
        ResponseTool::Function {
            name: self.name(),
            description: Some(self.description()),
            parameters: self.json_schema(),
            strict: false,
        }
    }
    fn json_schema(&self) -> serde_json::Value;
    fn description(&self) -> String;
    fn name(&self) -> String;
    async fn execute(&self, args: serde_json::Value, ob_ctx: ObserverContext) -> Result<String, String>;
} 