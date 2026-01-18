use cid::Cid;
use polyepoxide_core::{Bond, Cell, Oxide, Solvent, Store};
use polyepoxide_llm::{ContentBlock, GenerationParams, Message, MessageContent};
use silane_openrouter::{OpenRouterClient, OpenRouterError, OpenRouterRequest};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

use crate::error::SihError;
use crate::store::{AnyStore, AppContext};

const AVAILABLE_MODELS: &[&str] = &[
    "openai/gpt-4o",
    "openai/gpt-4o-mini",
    "openai/o1",
    "openai/o1-mini",
    "anthropic/claude-3.5-sonnet",
    "anthropic/claude-3-haiku",
    "google/gemini-2.0-flash-001",
    "google/gemini-2.0-flash-thinking-exp:free",
    "deepseek/deepseek-r1",
    "deepseek/deepseek-chat",
];

const REASONING_OPTIONS: &[Option<&str>] = &[None, Some("low"), Some("medium"), Some("high")];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Chat,
    SelectModel,
    SelectReasoning,
    Loading,
}

pub struct ChatApp {
    pub mode: AppMode,
    pub should_quit: bool,
    pub store: AnyStore,
    pub solvent: Solvent,
    pub conversation_head: Option<Arc<Cell<Message>>>,
    pub input: String,
    pub cursor_pos: usize,
    pub model: String,
    pub reasoning_effort: Option<String>,
    pub messages_scroll: u16,
    pub client: Arc<Mutex<OpenRouterClient>>,
    pub response_rx: Option<oneshot::Receiver<Result<Message, OpenRouterError>>>,
    pub last_error: Option<String>,

    // Popup state
    pub popup_selected: usize,
}

impl ChatApp {
    pub fn new(
        mut ctx: AppContext,
        client: OpenRouterClient,
        model: String,
        reasoning_effort: Option<String>,
        continue_from: Option<Cid>,
    ) -> Result<Self, SihError> {
        let conversation_head = if let Some(cid) = continue_from {
            Some(Self::load_conversation(&mut ctx.solvent, &ctx.store, &cid)?)
        } else {
            None
        };

        Ok(Self {
            mode: AppMode::Chat,
            should_quit: false,
            store: ctx.store,
            solvent: ctx.solvent,
            conversation_head,
            input: String::new(),
            cursor_pos: 0,
            model,
            reasoning_effort,
            messages_scroll: 0,
            client: Arc::new(Mutex::new(client)),
            response_rx: None,
            last_error: None,
            popup_selected: 0,
        })
    }

    fn load_conversation(
        solvent: &mut Solvent,
        store: &AnyStore,
        cid: &Cid,
    ) -> Result<Arc<Cell<Message>>, SihError> {
        Self::load_message_recursive(solvent, store, cid)
    }

    fn load_message_recursive(
        solvent: &mut Solvent,
        store: &AnyStore,
        cid: &Cid,
    ) -> Result<Arc<Cell<Message>>, SihError> {
        // Check if already loaded
        if let Some(cell) = solvent.get::<Message>(cid) {
            return Ok(cell);
        }

        // Load from store
        let bytes = store
            .get(cid)?
            .ok_or_else(|| SihError::MessageNotFound(*cid))?;

        let message: Message =
            Oxide::from_bytes(&bytes).map_err(|e| SihError::DecodeError(e.to_string()))?;

        // Recursively load previous messages first (to ensure they're in solvent)
        if let Some(ref prev_bond) = message.previous {
            Self::load_message_recursive(solvent, store, &prev_bond.cid())?;
        }

        // Now add this message to solvent (previous bonds will be resolved)
        Ok(solvent.add(message))
    }

    fn persist_message(&self, cell: &Cell<Message>) -> Result<(), SihError> {
        self.solvent.persist_cell(cell, &self.store)?;
        Ok(())
    }

    pub fn available_models() -> &'static [&'static str] {
        AVAILABLE_MODELS
    }

    pub fn reasoning_options() -> &'static [Option<&'static str>] {
        REASONING_OPTIONS
    }

    pub fn send_message(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }

        // Create user message
        let user_msg = Message {
            content: MessageContent::User(vec![ContentBlock::Text(text)]),
            metadata: None,
            previous: self.conversation_head.as_ref().map(|c| Bond::from_cell(Arc::clone(c))),
        };
        let user_cell = self.solvent.add(user_msg);

        // Persist user message to store
        if let Err(e) = self.persist_message(&user_cell) {
            self.last_error = Some(format!("Failed to persist message: {}", e));
            return;
        }

        self.conversation_head = Some(Arc::clone(&user_cell));

        // Clear input
        self.input.clear();
        self.cursor_pos = 0;

        // Prepare request
        let request = OpenRouterRequest {
            model: self.model.clone(),
            conversation_head: Bond::from_cell(Arc::clone(&user_cell)),
            params: self.reasoning_effort.as_ref().map(|effort| GenerationParams {
                temperature: None,
                top_p: None,
                top_k: None,
                max_tokens: None,
                frequency_penalty: None,
                presence_penalty: None,
                stop: None,
                min_p: None,
                top_a: None,
                repetition_penalty: None,
                seed: None,
                reasoning_effort: Some(effort.clone()),
                reasoning_max_tokens: None,
            }),
            tools: vec![],
            tool_choice: None,
        };

        // Spawn async task
        let (tx, rx) = oneshot::channel();
        let client = Arc::clone(&self.client);

        tokio::spawn(async move {
            let client = client.lock().await;
            let result = client.complete(&request).await;
            let _ = tx.send(result);
        });

        self.response_rx = Some(rx);
        self.mode = AppMode::Loading;
        self.last_error = None;
    }

    pub fn poll_response(&mut self) {
        if let Some(ref mut rx) = self.response_rx {
            match rx.try_recv() {
                Ok(Ok(message)) => {
                    let cell = self.solvent.add(message);

                    // Persist assistant message to store
                    if let Err(e) = self.persist_message(&cell) {
                        self.last_error = Some(format!("Failed to persist response: {}", e));
                    }

                    self.conversation_head = Some(cell);
                    self.response_rx = None;
                    self.mode = AppMode::Chat;
                    self.messages_scroll = 0; // Scroll to bottom
                }
                Ok(Err(e)) => {
                    self.last_error = Some(format!("{}", e));
                    self.response_rx = None;
                    self.mode = AppMode::Chat;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    // Still waiting
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    self.last_error = Some("Request cancelled".to_string());
                    self.response_rx = None;
                    self.mode = AppMode::Chat;
                }
            }
        }
    }

    pub fn open_model_picker(&mut self) {
        self.popup_selected = AVAILABLE_MODELS
            .iter()
            .position(|&m| m == self.model)
            .unwrap_or(0);
        self.mode = AppMode::SelectModel;
    }

    pub fn open_reasoning_picker(&mut self) {
        self.popup_selected = REASONING_OPTIONS
            .iter()
            .position(|o| o.map(String::from) == self.reasoning_effort)
            .unwrap_or(0);
        self.mode = AppMode::SelectReasoning;
    }

    pub fn close_popup(&mut self) {
        self.mode = AppMode::Chat;
    }

    pub fn popup_up(&mut self) {
        if self.popup_selected > 0 {
            self.popup_selected -= 1;
        }
    }

    pub fn popup_down(&mut self) {
        let max = match self.mode {
            AppMode::SelectModel => AVAILABLE_MODELS.len() - 1,
            AppMode::SelectReasoning => REASONING_OPTIONS.len() - 1,
            _ => 0,
        };
        if self.popup_selected < max {
            self.popup_selected += 1;
        }
    }

    pub fn popup_select(&mut self) {
        match self.mode {
            AppMode::SelectModel => {
                self.model = AVAILABLE_MODELS[self.popup_selected].to_string();
            }
            AppMode::SelectReasoning => {
                self.reasoning_effort = REASONING_OPTIONS[self.popup_selected].map(String::from);
            }
            _ => {}
        }
        self.close_popup();
    }

    pub fn scroll_up(&mut self) {
        self.messages_scroll = self.messages_scroll.saturating_add(1);
    }

    pub fn scroll_down(&mut self) {
        self.messages_scroll = self.messages_scroll.saturating_sub(1);
    }

    pub fn input_char(&mut self, c: char) {
        self.input.insert(self.cursor_pos, c);
        self.cursor_pos += c.len_utf8();
    }

    pub fn input_backspace(&mut self) {
        if self.cursor_pos > 0 {
            let prev_char_boundary = self.input[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(prev_char_boundary);
            self.cursor_pos = prev_char_boundary;
        }
    }

    pub fn input_delete(&mut self) {
        if self.cursor_pos < self.input.len() {
            self.input.remove(self.cursor_pos);
        }
    }

    pub fn input_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos = self.input[..self.cursor_pos]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn input_right(&mut self) {
        if self.cursor_pos < self.input.len() {
            self.cursor_pos = self.input[self.cursor_pos..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor_pos + i)
                .unwrap_or(self.input.len());
        }
    }

    pub fn input_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn input_end(&mut self) {
        self.cursor_pos = self.input.len();
    }

    pub fn conversation_cid(&self) -> Option<Cid> {
        self.conversation_head.as_ref().map(|c| c.cid())
    }

    pub fn get_messages(&self) -> Vec<&Message> {
        let mut messages = Vec::new();
        let mut current: Option<&Arc<Cell<Message>>> = self.conversation_head.as_ref();

        while let Some(cell) = current {
            messages.push(cell.value());
            current = cell
                .value()
                .previous
                .as_ref()
                .and_then(|b| b.cell());
        }

        messages.reverse();
        messages
    }
}

