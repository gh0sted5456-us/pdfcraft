# Application dialog windows

PdfCraft's popup windows share a thin frame, a centered title and minimize, maximize/restore and close controls. Titles can be dragged inside the application; double-click a title to maximize or restore it. Existing Save/Cancel/Exit actions keep their meaning and theme colors. Closing a dialog follows its existing cancellation path; minimizing never applies its changes.

The **Windows** taskbar sits inside PdfCraft. Select a minimized window to restore it, or use its X to close that draft. Minimize all parks the open windows. Main tool dialogs preserve their own draft fields, so opening Preferences does not replace an unfinished Properties or signing draft. A document-specific window returns to its source PDF. Up to 16 main dialogs can be parked; reaching the limit keeps the new dialog open rather than discarding anything.

The AI Assistant uses the whole current document workspace by default. Its chat and connections views are separate, with the message box kept beneath the conversation and document controls in their own pane. Restoring a minimized chat fills the current tab, including when another PDF is now active. Switching source clears PdfCraft's old reviewed text and native chat; it never automatically shares the new document. Browser chat keeps its hidden web page when minimized or when viewing connection settings. Conversations already pasted into a provider website remain under your control there; changing document tabs does not erase them. Active local requests are canceled on minimize, with an unsent question kept for resending; requests already transmitted may still be processed by the provider.

Minimized save confirmations remain pending. They do not approve Save, Exit or signing. Stored save/signing operations cannot resume if their source revision changed while hidden; PdfCraft keeps the document and asks the user to start that operation again. Other document drafts must be restored or closed before their source tab can close. Parked document dialogs stay with their native application window, so finish them before tearing that PDF into another window. Drafts are in memory only and are not recovery files or persisted conversations.

Native operating-system file pickers, security prompts and provider sign-in pages remain system/provider windows. Context menus and dropdowns remain ordinary transient menus.

## Opt-in UI control

The command registry exposes `app.dialog_minimize`, `app.dialog_restore` (one minimized window), `app.dialog_maximize`, and `app.dialog_minimize_all` through the existing `ui.command` control method. `ui.state.dialog_windows` reports window titles and minimized/maximized flags, never a draft, password, token or transcript. The authenticated loopback control channel still requires an explicit `--control` launch; the taskbar starts no server or AI request.
