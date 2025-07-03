use clap::Args;
use crossterm::style::{
    self,
    Color,
};
use crossterm::{
    execute,
    queue,
};
use dialoguer::Select;

use crate::api_client::config::QCliConfig;
use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::os::Os;

pub struct ModelOption {
    pub name: String,
    pub model_id: String,
    pub is_custom: bool,
}

// Keep backward compatibility
pub struct StaticModelOption {
    pub name: &'static str,
    pub model_id: &'static str,
}

pub const BUILTIN_MODEL_OPTIONS: [(&str, &str); 3] = [
    ("claude-4-sonnet", "CLAUDE_SONNET_4_20250514_V1_0"),
    ("claude-3.7-sonnet", "CLAUDE_3_7_SONNET_20250219_V1_0"),
    ("claude-3.5-sonnet", "CLAUDE_3_5_SONNET_20241022_V2_0"),
];

// For backward compatibility with existing code
pub const MODEL_OPTIONS: [StaticModelOption; 3] = [
    StaticModelOption {
        name: "claude-4-sonnet",
        model_id: "CLAUDE_SONNET_4_20250514_V1_0",
    },
    StaticModelOption {
        name: "claude-3.7-sonnet",
        model_id: "CLAUDE_3_7_SONNET_20250219_V1_0",
    },
    StaticModelOption {
        name: "claude-3.5-sonnet",
        model_id: "CLAUDE_3_5_SONNET_20241022_V2_0",
    },
];

pub fn get_all_model_options() -> Result<Vec<ModelOption>, ChatError> {
    let mut models = Vec::new();

    // Add built-in models
    for (name, model_id) in BUILTIN_MODEL_OPTIONS.iter() {
        models.push(ModelOption {
            name: (*name).to_string(),
            model_id: (*model_id).to_string(),
            is_custom: false,
        });
    }

    // Add custom models
    match QCliConfig::load() {
        Ok(config) => {
            if let Some(custom_models) = &config.custom_models {
                for (name, _custom_config) in custom_models.iter() {
                    models.push(ModelOption {
                        name: format!("custom:{}", name),
                        model_id: format!("custom:{}", name),
                        is_custom: true,
                    });
                }
            }
        },
        Err(e) => {
            // Log warning but don't fail - just continue with built-in models
            eprintln!("Warning: Could not load custom models: {}", e);
        },
    }

    Ok(models)
}

#[deny(missing_docs)]
#[derive(Debug, PartialEq, Args)]
pub struct ModelArgs;

impl ModelArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        queue!(session.stderr, style::Print("\n"))?;

        let model_options = get_all_model_options()?;
        let active_model_id = session.conversation.model.as_deref();

        let labels: Vec<String> = model_options
            .iter()
            .map(|opt| {
                if Some(opt.model_id.as_str()) == active_model_id {
                    format!("{} (active)", opt.name)
                } else {
                    opt.name.clone()
                }
            })
            .collect();

        let selection: Option<_> = match Select::with_theme(&crate::util::dialoguer_theme())
            .with_prompt("Select a model for this chat session")
            .items(&labels)
            .default(0)
            .interact_on_opt(&dialoguer::console::Term::stdout())
        {
            Ok(sel) => {
                let _ = crossterm::execute!(
                    std::io::stdout(),
                    crossterm::style::SetForegroundColor(crossterm::style::Color::Magenta)
                );
                sel
            },
            // Ctrl‑C -> Err(Interrupted)
            Err(dialoguer::Error::IO(ref e)) if e.kind() == std::io::ErrorKind::Interrupted => None,
            Err(e) => return Err(ChatError::Custom(format!("Failed to choose model: {e}").into())),
        };

        queue!(session.stderr, style::ResetColor)?;

        if let Some(index) = selection {
            let selected = &model_options[index];
            session.conversation.model = Some(selected.model_id.clone());

            let display_name = if selected.is_custom {
                format!("{} (Custom Model)", selected.name)
            } else {
                selected.name.clone()
            };

            queue!(
                session.stderr,
                style::Print("\n"),
                style::Print(format!(" Using {}\n\n", display_name)),
                style::ResetColor,
                style::SetForegroundColor(Color::Reset),
                style::SetBackgroundColor(Color::Reset),
            )?;
        }

        execute!(session.stderr, style::ResetColor)?;

        Ok(ChatState::PromptUser {
            skip_printing_tools: false,
        })
    }
}

/// Currently, Sonnet 4 is set as the default model for non-FRA users.
pub fn default_model_id(os: &Os) -> &'static str {
    match os.database.get_auth_profile() {
        Ok(Some(profile)) if profile.arn.split(':').nth(3) == Some("eu-central-1") => "CLAUDE_3_7_SONNET_20250219_V1_0",
        _ => "CLAUDE_SONNET_4_20250514_V1_0",
    }
}
