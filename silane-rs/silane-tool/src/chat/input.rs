use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

use super::app::{AppMode, ChatApp};

pub fn handle_event(app: &mut ChatApp, event: Event) {
    if let Event::Key(key) = event {
        handle_key(app, key);
    }
}

fn handle_key(app: &mut ChatApp, key: KeyEvent) {
    match app.mode {
        AppMode::Chat => handle_chat_key(app, key),
        AppMode::Loading => handle_loading_key(app, key),
        AppMode::SelectModel | AppMode::SelectReasoning => handle_popup_key(app, key),
    }
}

fn handle_chat_key(app: &mut ChatApp, key: KeyEvent) {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) => {
            app.should_quit = true;
        }
        (KeyCode::F(2), _) => {
            app.open_model_picker();
        }
        (KeyCode::F(3), _) => {
            app.open_reasoning_picker();
        }
        (KeyCode::Enter, KeyModifiers::NONE) => {
            app.send_message();
        }
        (KeyCode::Up, KeyModifiers::CONTROL) => {
            app.scroll_up();
        }
        (KeyCode::Down, KeyModifiers::CONTROL) => {
            app.scroll_down();
        }
        (KeyCode::Backspace, _) => {
            app.input_backspace();
        }
        (KeyCode::Delete, _) => {
            app.input_delete();
        }
        (KeyCode::Left, _) => {
            app.input_left();
        }
        (KeyCode::Right, _) => {
            app.input_right();
        }
        (KeyCode::Home, _) => {
            app.input_home();
        }
        (KeyCode::End, _) => {
            app.input_end();
        }
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            app.input_char(c);
        }
        _ => {}
    }
}

fn handle_loading_key(app: &mut ChatApp, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        app.response_rx = None;
        app.mode = AppMode::Chat;
        app.last_error = Some("Request cancelled".to_string());
    }
}

fn handle_popup_key(app: &mut ChatApp, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => app.close_popup(),
        KeyCode::Enter => app.popup_select(),
        KeyCode::Up => app.popup_up(),
        KeyCode::Down => app.popup_down(),
        _ => {}
    }
}
