//! Scripted provider for tests and offline demos: replays a fixed sequence of responses
//! and records every request it receives. No network.

use crate::message::{CancelToken, ChatRequest, ChatResponse, ToolCall};
use crate::provider::{Provider, ProviderError};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Hook run inside `complete`, e.g. to press Cancel mid-request.
pub type HookFn = dyn Fn(&CancelToken) -> Result<ChatResponse, ProviderError> + Send;

/// One scripted step: a response, an error, or a hook that runs during the call (e.g. to
/// cancel the token mid-request) before returning its response.
pub enum Step {
    Respond(ChatResponse),
    Fail(ProviderError),
    Hook(Box<HookFn>),
}

#[derive(Clone, Default)]
pub struct ScriptedProvider {
    steps: Arc<Mutex<VecDeque<Step>>>,
    requests: Arc<Mutex<Vec<ChatRequest>>>,
}

impl ScriptedProvider {
    pub fn new(steps: Vec<Step>) -> ScriptedProvider {
        ScriptedProvider {
            steps: Arc::new(Mutex::new(steps.into())),
            requests: Arc::default(),
        }
    }
    /// Requests received so far (shared with clones handed to an agent).
    pub fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().expect("lock").clone()
    }
    pub fn remaining(&self) -> usize {
        self.steps.lock().expect("lock").len()
    }
}

impl Provider for ScriptedProvider {
    fn label(&self) -> String {
        "scripted-mock".into()
    }
    fn complete(
        &self,
        request: &ChatRequest,
        cancel: &CancelToken,
    ) -> Result<ChatResponse, ProviderError> {
        self.requests.lock().expect("lock").push(request.clone());
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let step = self.steps.lock().expect("lock").pop_front();
        match step {
            Some(Step::Respond(r)) => Ok(r),
            Some(Step::Fail(e)) => Err(e),
            Some(Step::Hook(f)) => f(cancel),
            None => Err(ProviderError::Exhausted(
                "the script has no more responses".into(),
            )),
        }
    }
}

/// Build a tool call for scripts.
pub fn call(id: &str, name: &str, arguments: Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments,
    }
}
