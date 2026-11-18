//! Lion PDF: Help ▸ Connect AI assistants…, the settings that let an AI client (Claude Desktop,
//! Claude Code, Cursor…) create and edit PDFs through Lion PDF's MCP server.
//!
//! The dialog only shows text to copy. Lion PDF never starts the server or edits a client's
//! settings (AGENTS.md §3): the client starts `pdfcraft-cli mcp` itself once the person adds them.
//! `pdfcraft-cli mcp-setup` prints the same settings.

use std::path::PathBuf;

use pdfcraft_engine::ai_connect::{Connection, Shell};

use crate::{PdfCraftApp, theme, widgets};

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

/// One block of settings to copy, with its own Copy button.
fn copy_block(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &theme::Tokens, id: &str, text: &str) {
    let mut shown = text;
    ui.add(egui::TextEdit::multiline(&mut shown).id_salt(id).code_editor().desired_rows(1).desired_width(f32::INFINITY).background_color(t.field));
    if ui.button(tl!("Copy")).clicked() {
        ui.ctx().copy_text(text.to_owned());
        app.notify_tr("Copied");
    }
}

/// The dialog's content. Returns whether it should close.
pub(crate) fn body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &theme::Tokens) -> bool {
    widgets::dialog_heading(ui, tl!("Connect AI assistants"));
    ui.set_width(640.0_f32.min((ui.ctx().content_rect().width() - 104.0).max(1.0)));
    let mut choose = false;
    widgets::dialog_scroll(ui, "ai-connect-body", |ui| {
        ui.label(tl!(
            "An AI assistant such as Claude can create, fill in, edit and save PDFs with Lion PDF. Add the settings below to your AI app: it then runs Lion PDF's command-line tool itself. Lion PDF starts nothing and changes no other app's settings."
        ));
        ui.add_space(10.0);
        let Some(connection) = app.ai_connection() else {
            ui.label(tl!("Connecting AI assistants needs the desktop app."));
            return;
        };
        ui.horizontal_wrapped(|ui| {
            let label = ui.label(tl!("Folder the assistant can use:"));
            ui.label(egui::RichText::new(connection.folder.display().to_string()).monospace()).labelled_by(label.id);
            choose = ui.button(tl!("Choose…")).clicked();
        });
        ui.label(egui::RichText::new(tl!("The assistant can open and save files in this folder only.")).color(t.text_muted));
        if !connection.cli.is_file() {
            ui.add_space(6.0);
            ui.colored_label(
                ui.visuals().error_fg_color,
                tl!("pdfcraft-cli isn't next to Lion PDF. Install Lion PDF with its installer, or change \"command\" to where pdfcraft-cli is."),
            );
        }
        ui.add_space(12.0);
        widgets::section_title(ui, tl!("Claude Desktop"));
        let config =
            pdfcraft_engine::ai_connect::claude_desktop_config().map_or_else(|| "claude_desktop_config.json".to_owned(), |p| p.display().to_string());
        ui.label(crate::i18n::fmt(
            tl!("In Claude, choose Settings ▸ Developer ▸ Edit Config, add this to {file}, then restart Claude:"),
            &[("file", &config)],
        ));
        copy_block(ui, app, t, "ai-connect-json", &connection.json());
        ui.add_space(12.0);
        widgets::section_title(ui, tl!("Claude Code"));
        ui.label(tl!("Run this once in a terminal:"));
        copy_block(ui, app, t, "ai-connect-claude-code", &connection.claude_code(Shell::native()));
        ui.add_space(12.0);
        widgets::section_title(ui, tl!("Other AI apps"));
        ui.label(tl!("Cursor, VS Code and other MCP clients take the same \"mcpServers\" settings as Claude Desktop."));
    });
    if choose {
        app.ask_one(crate::pickers::Ask::Folder(rfd::AsyncFileDialog::new()), None, |app, dir| app.ai_connect_folder = Some(dir));
    }
    ui.add_space(12.0);
    let mut close = false;
    widgets::dialog_footer(ui, |ui| {
        close = widgets::dialog_button(ui, tl!("Done"), widgets::DialogAction::Confirm).clicked();
    });
    close
}
