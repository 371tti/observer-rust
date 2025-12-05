use dashmap::DashMap;
use serenity::all::UserId;

use crate::config::Models;

/// ユーザー情報のプール
pub struct UserContexts {
    pub contexts: DashMap<UserId, UserContext>,
}

/// ユーザー情報の構造体
#[derive(Clone)]
pub struct UserContext {
    pub user_id: UserId,
    pub main_model: Models,
    pub rate_line: u64,
}

impl UserContext {
    pub fn new(user_id: UserId) -> UserContext {
        UserContext {
            user_id,
            main_model: Models::O4Mini,
            rate_line: 1,
        }
    }
}

impl UserContexts {
    pub fn new() -> UserContexts {
        UserContexts {
            contexts: DashMap::new(),
        }
    }

    pub fn get_or_create(&self, user_id: UserId) -> UserContext {
        self.contexts
            .entry(user_id)
            .or_insert_with(|| UserContext::new(user_id))
            .clone()
    }

    pub fn set_model(&self, user_id: UserId, model: Models) {
        self.contexts
            .entry(user_id)
            .or_insert_with(|| UserContext::new(user_id))
            .main_model = model;
    }

    pub fn set_rate_line(&self, user_id: UserId, rate_line: u64) {
        self.contexts
            .entry(user_id)
            .or_insert_with(|| UserContext::new(user_id))
            .rate_line = rate_line;
    }
}