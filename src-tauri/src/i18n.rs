//! User-interface language. Portuguese (Brazil) is the source language: every
//! user-visible string in the code is written in Portuguese, and `tr` looks the
//! text up in the English table when English is selected. A string with no
//! English entry is shown as written, so a missing translation can never blank
//! the UI (and `every_literal_has_an_english_entry` fails the build of tests).
//!
//! The language lives in `settings.json` (`language`); `set` mirrors it into a
//! process-wide atomic so lookups never touch the disk.

use std::borrow::Cow;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    PtBr,
    En,
}

pub const PT_BR: &str = "pt-BR";
pub const EN: &str = "en";

impl Lang {
    pub fn from_code(code: &str) -> Lang {
        match code.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en-gb" => Lang::En,
            _ => Lang::PtBr,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::PtBr => PT_BR,
            Lang::En => EN,
        }
    }
}

/// Normalise any stored value to one of the supported codes.
pub fn sanitize_code(code: &str) -> String {
    Lang::from_code(code).code().to_string()
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::PtBr
    }
}

fn english(s: &str) -> Option<&'static str> {
    EN_TABLE.iter().find(|(pt, _)| *pt == s).map(|(_, en)| *en)
}

/// Translate a Portuguese source string into the current language.
pub fn tr(s: &str) -> Cow<'_, str> {
    tr_in(current(), s)
}

pub fn tr_in(lang: Lang, s: &str) -> Cow<'_, str> {
    match lang {
        Lang::PtBr => Cow::Borrowed(s),
        Lang::En => english(s).map_or(Cow::Borrowed(s), Cow::Borrowed),
    }
}

/// Translate a pattern containing `{0}`, `{1}`... placeholders, then fill them.
pub fn trf(pattern: &str, args: &[&dyn std::fmt::Display]) -> String {
    trf_in(current(), pattern, args)
}

pub fn trf_in(lang: Lang, pattern: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = tr_in(lang, pattern).into_owned();
    for (i, a) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), &a.to_string());
    }
    out
}

/// Re-apply the language everywhere it is already on screen: open pages, the
/// tray menu and tooltip, the native window menus and the settings title.
pub fn refresh(app: &tauri::AppHandle) {
    use tauri::Manager;
    for (_, wv) in app.webviews() {
        let _ = wv.eval(switch_script());
    }
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.set_title(&tr("whatRust — Configurações"));
    }
    crate::tray::rebuild_menu(app);
    let total = app
        .try_state::<crate::accounts::UnreadMap>()
        .map(|m| crate::accounts::aggregate_unread(&crate::lock::guard(&m)))
        .unwrap_or(0);
    crate::tray::update_badge(app, total);
    #[cfg(windows)]
    crate::sessions::refresh_menus(app);
}

/// Script run in every local page before its own scripts: tells `i18n.js` the
/// language to start in.
pub fn init_script() -> String {
    format!("window.__WR_LANG={:?};", current().code())
}

/// Script that switches an already-open page.
pub fn switch_script() -> String {
    format!(
        "window.__WR_LANG={:?};window.I18n&&I18n.setLang({:?});window.__wrRelang&&window.__wrRelang();",
        current().code(),
        current().code()
    )
}

/// English text for each Portuguese source string. Placeholders are `{0}`, `{1}`.
static EN_TABLE: &[(&str, &str)] = &[
    // accounts
    ("Não é possível remover a última conta.", "The last account cannot be removed."),
    ("Conta não encontrada: {0}", "Account not found: {0}"),
    ("Conta não encontrada", "Account not found"),
    ("A ordem deve conter todas as contas uma única vez", "The order must contain every account exactly once"),
    ("Conta não está aberta", "The account is not open"),
    ("{0}; e não foi possível restaurar a conta: {1}", "{0}; and the account could not be restored: {1}"),
    // lock / password
    ("As senhas não coincidem.", "The passwords do not match."),
    ("A senha deve ter pelo menos {0} caracteres.", "The password must be at least {0} characters long."),
    ("O bloqueio já está ativado.", "The lock is already on."),
    ("O bloqueio não está ativado.", "The lock is not on."),
    ("A senha atual está incorreta.", "The current password is incorrect."),
    ("Defina uma senha de bloqueio primeiro.", "Set a lock password first."),
    ("A autenticação biométrica não está disponível neste dispositivo.", "Biometric authentication is not available on this device."),
    ("Ativar desbloqueio por biometria no whatRust", "Turn on biometric unlock in whatRust"),
    ("Desbloquear o whatRust", "Unlock whatRust"),
    ("Não foi possível confirmar a biometria.", "Biometrics could not be confirmed."),
    ("O desbloqueio por biometria não está ativado.", "Biometric unlock is not on."),
    ("O tempo para confirmar a biometria terminou.", "Time to confirm biometrics ran out."),
    ("A redefinição precisa ser confirmada.", "The reset must be confirmed."),
    ("Não foi possível concluir a redefinição. Feche outras instâncias do whatRust e tente novamente.", "The reset could not be completed. Close other instances of whatRust and try again."),
    ("whatRust — Bloqueado", "whatRust — Locked"),
    // downloads / files
    ("Download não encontrado", "Download not found"),
    ("Download concluído", "Download complete"),
    ("Falha no download", "Download failed"),
    ("{0} — salvo no local escolhido.", "{0} — saved to the chosen location."),
    ("Arquivo salvo no local escolhido.", "File saved to the chosen location."),
    ("Não foi possível baixar o arquivo. Tente novamente.", "The file could not be downloaded. Try again."),
    ("Não foi possível abrir o link", "Could not open the link"),
    ("Não foi possível iniciar o navegador. Copie o link e abra-o no navegador.", "Could not start the browser. Copy the link and open it in your browser."),
    ("Não foi possível abrir o arquivo", "Could not open the file"),
    ("Abra o arquivo pela pasta em que ele foi salvo.", "Open the file from the folder where it was saved."),
    // drag and drop
    ("Nenhum arquivo foi anexado", "No file was attached"),
    ("Não é possível arrastar esse item diretamente. Arraste o arquivo pelo Explorador de Arquivos ou use o botão de anexo (+).", "This item cannot be dragged directly. Drag the file from File Explorer or use the attach button (+)."),
    ("Arquivos não anexados", "Files not attached"),
    ("Não foi possível processar os arquivos. Tente novamente.", "The files could not be processed. Try again."),
    ("Não foi possível enviar os arquivos ao WhatsApp. Tente novamente.", "The files could not be sent to WhatsApp. Try again."),
    ("O WhatsApp ainda não estava pronto para receber os arquivos. Tente novamente.", "WhatsApp was not ready to receive the files. Try again."),
    ("Alguns arquivos não foram anexados", "Some files were not attached"),
    ("arquivo", "file"),
    ("arquivos", "files"),
    ("{0} {1} acima do limite de 500 MB", "{0} {1} over the 500 MB limit"),
    ("{0} {1} acima do total de 500 MB por envio", "{0} {1} over the 500 MB total per upload"),
    ("{0} {1} acima do limite de 200 arquivos", "{0} {1} over the limit of 200 files"),
    ("pastas ou arquivos especiais: {0}", "folders or special files: {0}"),
    ("{0} {1} sem permissão de leitura", "{0} {1} without read permission"),
    ("{0} {1} alterados durante a leitura", "{0} {1} changed while reading"),
    ("Não anexados: {0}.", "Not attached: {0}."),
    // notifications
    ("Nova mensagem", "New message"),
    ("Você tem uma nova mensagem.", "You have a new message."),
    ("Você tem novas mensagens em {0} conversas.", "You have new messages in {0} chats."),
    ("Você recebeu uma mensagem no WhatsApp.", "You received a message on WhatsApp."),
    ("Teste do whatRust", "whatRust test"),
    ("Se você está vendo este aviso, o Windows exibiu o teste.", "If you can see this notice, Windows displayed the test."),
    ("Ative as notificações antes do teste.", "Turn on notifications before the test."),
    ("Teste solicitado. Confira o banner e a central do Windows; aceitação não confirma exibição.", "Test requested. Check the banner and the Windows notification center; acceptance does not confirm it was shown."),
    ("Não foi possível abrir as configurações do Windows", "Could not open the Windows settings"),
    ("Disponível no Windows", "Available on Windows"),
    // settings side effects
    ("Não foi possível atualizar a inicialização automática: {0}", "Could not update automatic startup: {0}"),
    ("Não foi possível registrar o atalho (ele pode estar em uso): {0}", "Could not register the shortcut (it may be in use): {0}"),
    // tray and menus
    ("Contas…", "Accounts…"),
    ("Contas", "Accounts"),
    ("Configurações", "Settings"),
    ("Recarregar", "Reload"),
    ("Sair", "Quit"),
    ("Bloquear agora", "Lock now"),
    ("Bloquear", "Lock"),
    ("{0} conversas não lidas — WhatsApp", "{0} unread chats — WhatsApp"),
    ("Configurações do whatRust", "whatRust settings"),
    ("Voltar ao WhatsApp", "Back to WhatsApp"),
    ("Recarregar conta", "Reload account"),
    ("Sair do whatRust", "Quit whatRust"),
    // window titles
    ("whatRust — Configurações", "whatRust — Settings"),
    ("whatRust — Conta destacada", "whatRust — Detached account"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portuguese_is_the_identity() {
        assert_eq!(tr_in(Lang::PtBr, "Sair"), "Sair");
    }

    #[test]
    fn english_translates_and_falls_back() {
        assert_eq!(tr_in(Lang::En, "Sair"), "Quit");
        assert_eq!(tr_in(Lang::En, "texto sem tradução"), "texto sem tradução");
    }

    #[test]
    fn placeholders_are_filled_after_translation() {
        assert_eq!(
            trf_in(Lang::En, "Conta não encontrada: {0}", &[&"acct-1"]),
            "Account not found: acct-1"
        );
        assert_eq!(
            trf_in(
                Lang::PtBr,
                "{0} {1} acima do limite de 500 MB",
                &[&2, &"arquivos"]
            ),
            "2 arquivos acima do limite de 500 MB"
        );
    }

    #[test]
    fn codes_round_trip_and_unknown_means_portuguese() {
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("EN-us"), Lang::En);
        assert_eq!(Lang::from_code("pt-BR"), Lang::PtBr);
        assert_eq!(Lang::from_code("fr"), Lang::PtBr);
        assert_eq!(sanitize_code("zz"), PT_BR);
        assert_eq!(sanitize_code("en"), EN);
    }

    #[test]
    fn the_table_has_no_duplicate_keys_and_keeps_placeholders() {
        for (i, (pt, en)) in EN_TABLE.iter().enumerate() {
            assert!(
                EN_TABLE[i + 1..].iter().all(|(other, _)| other != pt),
                "duplicate key {pt}"
            );
            for n in 0..4 {
                let ph = format!("{{{n}}}");
                assert_eq!(
                    pt.contains(&ph),
                    en.contains(&ph),
                    "placeholder {ph} in {pt}"
                );
            }
        }
    }

    /// Every `tr("…")` / `trf("…", …)` literal in the sources must have an English
    /// entry, so adding a string without translating it fails here.
    #[test]
    fn every_literal_has_an_english_entry() {
        let sources = [
            include_str!("accounts.rs"),
            include_str!("applock.rs"),
            include_str!("commands.rs"),
            include_str!("integration.rs"),
            include_str!("lock.rs"),
            include_str!("notify.rs"),
            include_str!("opener.rs"),
            include_str!("sessions.rs"),
            include_str!("settings.rs"),
            include_str!("tray.rs"),
            include_str!("window.rs"),
            include_str!("biometric/macos.rs"),
        ];
        let mut missing = Vec::new();
        let mut found = 0;
        for src in sources {
            let mut rest = src;
            while let Some(i) = [rest.find("tr(\""), rest.find("trf(\"")]
                .into_iter()
                .flatten()
                .min()
            {
                let after = &rest[i..];
                let start = after.find('"').unwrap() + 1;
                let body = &after[start..];
                // Skip escaped quotes.
                let mut end = 0;
                let bytes = body.as_bytes();
                while end < bytes.len()
                    && !(bytes[end] == b'"' && (end == 0 || bytes[end - 1] != b'\\'))
                {
                    end += 1;
                }
                let lit = body[..end].replace("\\u{2026}", "…").replace("\\\"", "\"");
                // Only standalone calls: `tr(`/`trf(` not part of a longer identifier.
                let before = rest[..i].chars().last().unwrap_or(' ');
                if !(before.is_alphanumeric() || before == '_') {
                    found += 1;
                    if english(&lit).is_none() {
                        missing.push(lit);
                    }
                }
                rest = &after[start + end..];
            }
        }
        assert!(found > 20, "scanner found only {found} literals");
        assert!(missing.is_empty(), "no English entry for: {missing:#?}");
    }
}
