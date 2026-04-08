use dashmap::DashMap;
use serenity::all::ChannelId;

use crate::lmclient::LMContext;

/// チャンネルごとのプール
pub struct ChatContexts {
    pub contexts: DashMap<ChannelId, ChatContext>,
    pub default_system_prompt: String,
}

/// チャンネルごとのデータ保持
pub struct ChatContext {
    pub channel_id: ChannelId,
    pub context: LMContext,
    pub system_prompt: Option<String>,
    pub enabled: bool,
}

impl ChatContext {
    pub fn new(channel_id: ChannelId) -> ChatContext {
        ChatContext {
            channel_id,
            context: LMContext::new(),
            system_prompt: None,
            enabled: false,
        }
    }
}

impl ChatContexts {
    pub fn new(default_system_prompt: String) -> ChatContexts {
        ChatContexts {
            contexts: DashMap::new(),
            default_system_prompt,
        }
    }

    pub fn get_or_create(&self, channel_id: ChannelId) -> LMContext {
        self.contexts
            .entry(channel_id)
            .or_insert_with(|| ChatContext::new(channel_id))
            .context
            .clone()
    }

    pub fn get_system_prompt(&self, channel_id: ChannelId) -> String {
        self.contexts
            .get(&channel_id)
            .and_then(|entry| entry.system_prompt.clone())
            .unwrap_or_else(|| self.default_system_prompt.clone())
    }

    pub fn set_system_prompt(&self, channel_id: ChannelId, system_prompt: Option<String>) {
        let mut entry = self
            .contexts
            .entry(channel_id)
            .or_insert_with(|| ChatContext::new(channel_id));
        entry.system_prompt = system_prompt;
    }

    pub fn marge(&self, channel_id: ChannelId, other: &LMContext) {
        if let Some(mut entry) = self.contexts.get_mut(&channel_id) {
            entry.context.extend(other);
        } else {
            let mut new_context = LMContext::new();
            new_context.extend(other);
            self.contexts.insert(
                channel_id,
                ChatContext {
                    channel_id,
                    context: new_context,
                    system_prompt: None,
                    enabled: false,
                },
            );
        }
    }

    pub fn get_mut(&self, channel_id: ChannelId) -> Option<LMContext> {
        self.contexts.get(&channel_id).map(|entry| entry.context.clone())
    }

    pub fn is_enabled(&self, channel_id: ChannelId) -> bool {
        self.contexts
            .get(&channel_id)
            .map(|entry| entry.enabled)
            .unwrap_or(false)
    }

    pub fn clear(&self, channel_id: ChannelId) {
        if let Some(mut entry) = self.contexts.get_mut(&channel_id) {
            entry.context.clear();
        }
    }

    pub fn set_enabled(&self, channel_id: ChannelId, enabled: bool) {
        let mut entry = self
            .contexts
            .entry(channel_id)
            .or_insert_with(|| ChatContext::new(channel_id));
        entry.enabled = enabled;
    }
}