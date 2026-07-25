use crate::App;
use crate::View;

pub fn show(
    app: &mut App,
    ui: &mut egui::Ui,
    username_input: &mut String,
    login_state: &mut LoginState,
) {
    if ui.button("Back").clicked() {
        app.current_view = View::AccountList;
        *login_state = LoginState::Idle;
        return;
    }

    ui.heading("Add Account");

    match login_state {
        LoginState::Idle => {
            ui.label("Enter a username for offline play:");
            ui.text_edit_singleline(username_input);

            if ui.button("Add Offline Account").clicked() && !username_input.is_empty() {
                let account = release_the_launcher_auth::AccountData::offline(username_input);
                app.account_list.add(account);
                let _ = app.account_list.save();
                app.status_message = format!("Added offline account: {username_input}");
                app.current_view = View::AccountList;
                *login_state = LoginState::Idle;
            }

            ui.separator();
            ui.label("Or sign in with Microsoft:");
            if ui.button("Microsoft Login").clicked() {
                *login_state = LoginState::MicrosoftPending;
                app.status_message =
                    "Microsoft login requires a browser. Use the device code flow.".to_string();
            }
        }
        LoginState::MicrosoftPending => {
            ui.label("Microsoft login via device code flow.");
            ui.label("Check the terminal for the device code and URL.");
            if ui.button("Cancel").clicked() {
                *login_state = LoginState::Idle;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoginState {
    Idle,
    MicrosoftPending,
}
