//! Lion PDF: Help ▸ Connect AI assistants…, the settings that let an AI client (Claude Desktop,
//! Claude Code, Cursor…) create and edit PDFs through Lion PDF's MCP server.
//!
//! The dialog only shows text to copy. Lion PDF never starts the server or edits a client's
//! settings (AGENTS.md §3): the client starts `pdfcraft-cli mcp` itself once the person adds them.
//! `pdfcraft-cli mcp-setup` prints the same settings.

use std::path::PathBuf;

use pdfcraft_engine::ai_connect::{Connection, Shell};

use craft::host::PickRequest;
use craft::ui::contract::Bind;
use craft::ui::patterns::form_dialogs::{FormButtons, FormDialog};
use craft::ui::sections::{DialogRole, FooterButton};

use crate::PdfCraftApp;

/// The folder offered before the person chooses one: Documents when there is one, else home.
fn default_folder() -> Option<PathBuf> {
    let home = ["USERPROFILE", "HOME"].iter().find_map(|k| std::env::var_os(k).filter(|v| !v.is_empty())).map(PathBuf::from)?;
    let documents = home.join("Documents");
    Some(if documents.is_dir() { documents } else { home })
}

/// The `pdfcraft-cli` beside this app; `None` where there is no program file (the web build).
fn cli() -> Option<PathBuf> {
    std::env::current_exe().ok().map(|exe| Connection::cli_beside(&exe))
}

impl PdfCraftApp {
    /// The connection the dialog shows, if the app knows where it and the folder are.
    pub(crate) fn ai_connection(&self) -> Option<Connection> {
        let folder = self.ai_connect_folder.clone().or_else(default_folder)?;
        Some(Connection::new(cli()?, folder))
    }
}

/// One block of settings to copy, with its own Copy button. Returns whether Copy was pressed.
fn copy_block(ui: &mut egui::Ui, id: &str, text: &str) -> bool {
    let mut shown = text;
    ui.add(egui::TextEdit::multiline(&mut shown).id_salt(id).code_editor().desired_rows(1).desired_width(f32::INFINITY));
    if ui.button(tl!("Copy")).clicked() {
        ui.ctx().copy_text(text.to_owned());
        return true;
    }
    false
}

/// The footer button after Done.
const CHOOSE: usize = 2;

impl PdfCraftApp {
    /// The folder Connect AI assistants offers the assistant (tests and automation choose it ahead).
    pub fn set_ai_connect_folder(&mut self, folder: PathBuf) {
        self.ai_connect_folder = Some(folder);
    }

    /// Help ▸ Connect AI assistants…
    pub(crate) fn open_ai_connect(&mut self) {
        self.ai_connect_open = true;
    }

    /// One pass of the open Connect AI assistants dialog.
    pub(crate) fn ai_connect_pass(&mut self, ui: &mut egui::Ui) {
        if !self.ai_connect_open {
            return;
        }
        let connection = self.ai_connection();
        let mut open = true;
        let mut copied = false;
        let mut unit = ();
        let mut dialog = FormDialog::new(tl!("Connect AI assistants"), Bind::bound(&mut unit))
            .opened(Bind::bound(&mut open))
            .buttons(FormButtons::Close);
        if connection.is_some() {
            dialog = dialog.extra_buttons([FooterButton::new(tl!("Choose folder…"), DialogRole::Other)]);
        }
        let out = dialog.show(ui, |ui, _, _| {
            ui.set_max_width(640.0);
            ui.add(egui::Label::new(tl!(
                "An AI assistant such as Claude can create, fill in, edit and save PDFs with Lion PDF. Add the settings below to your AI app: it then runs Lion PDF's command-line tool itself. Lion PDF starts nothing and changes no other app's settings."
            )).wrap());
            ui.add_space(10.0);
            let Some(connection) = &connection else {
                ui.label(tl!("Connecting AI assistants needs the desktop app."));
                return true;
            };
            ui.horizontal_wrapped(|ui| {
                let label = ui.label(tl!("Folder the assistant can use:"));
                ui.label(egui::RichText::new(connection.folder.display().to_string()).monospace()).labelled_by(label.id);
            });
            ui.label(egui::RichText::new(tl!("The assistant can open and save files in this folder only.")).weak());
            if !connection.cli.is_file() {
                ui.add_space(6.0);
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    tl!("pdfcraft-cli isn't next to Lion PDF. Install Lion PDF with its installer, or change \"command\" to where pdfcraft-cli is."),
                );
            }
            ui.add_space(12.0);
            ui.label(egui::RichText::new(tl!("Claude Desktop")).strong());
            let config = pdfcraft_engine::ai_connect::claude_desktop_config()
                .map_or_else(|| "claude_desktop_config.json".to_owned(), |p| p.display().to_string());
            ui.add(egui::Label::new(crate::i18n::fmt(
                tl!("In Claude, choose Settings > Developer > Edit Config, add this to {file}, then restart Claude:"),
                &[("file", &config)],
            )).wrap());
            copied |= copy_block(ui, "ai-connect-json", &connection.json());
            ui.add_space(12.0);
            ui.label(egui::RichText::new(tl!("Claude Code")).strong());
            ui.label(tl!("Run this once in a terminal:"));
            copied |= copy_block(ui, "ai-connect-claude-code", &connection.claude_code(Shell::native()));
            ui.add_space(12.0);
            ui.label(egui::RichText::new(tl!("Other AI apps")).strong());
            ui.add(egui::Label::new(tl!("Cursor, VS Code and other MCP clients take the same \"mcpServers\" settings as Claude Desktop.")).wrap());
            true
        });
        if copied {
            self.notify_tr("Copied");
        }
        if out.closed.is_some() {
            self.ai_connect_open = false;
            return;
        }
        if out.picked == Some(CHOOSE) {
            self.pick_file(PickRequest::folder("ai.connect_folder").title(tl!("Folder the assistant can use")), None, |app, _, picked| {
                if let Some(dir) = picked.first_path() {
                    app.ai_connect_folder = Some(dir);
                }
            });
        }
    }
}
