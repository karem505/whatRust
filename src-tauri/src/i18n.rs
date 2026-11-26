//! User-interface language. Portuguese (Brazil) is the source language: every
//! user-visible string in the code is written in Portuguese, and `tr` looks the
//! text up in the English or Spanish table when that language is selected. A
//! Spanish string with no entry falls back to English, and one with no English
//! entry is shown as written, so a missing translation can never blank the UI
//! (and `every_literal_has_an_english_entry` / `spanish_covers_every_english_entry`
//! fail the tests).
//!
//! The language lives in `settings.json` (`language`); `set` mirrors it into a
//! process-wide atomic so lookups never touch the disk.

use std::borrow::Cow;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    PtBr,
    En,
    Es,
    Ar,
    Id,
}

pub const PT_BR: &str = "pt-BR";
pub const EN: &str = "en";
pub const ES: &str = "es";
pub const AR: &str = "ar";
pub const ID: &str = "id";

impl Lang {
    /// `pt`, `pt-BR`, `pt_PT.UTF-8`... mean Portuguese; `es`, `es-CL`,
    /// `es_ES.UTF-8`... mean Spanish; `ar`, `ar-EG`... mean Arabic; `id`,
    /// `id_ID.UTF-8`... mean Indonesian; anything else, including an empty or
    /// unknown code, means English.
    pub fn from_code(code: &str) -> Lang {
        let code = code.trim().to_ascii_lowercase();
        let is = |p: &str| {
            code == p
                || code
                    .strip_prefix(p)
                    .is_some_and(|rest| rest.starts_with(['-', '_']))
        };
        if is("pt") {
            Lang::PtBr
        } else if is("es") {
            Lang::Es
        } else if is("ar") {
            Lang::Ar
        } else if is("id") {
            Lang::Id
        } else {
            Lang::En
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::PtBr => PT_BR,
            Lang::En => EN,
            Lang::Es => ES,
            Lang::Ar => AR,
            Lang::Id => ID,
        }
    }
}

/// Normalise any stored value to one of the supported codes.
pub fn sanitize_code(code: &str) -> String {
    Lang::from_code(code).code().to_string()
}

/// Default for a fresh install or a settings.json written before the language
/// setting existed: Portuguese, Spanish, Arabic or Indonesian when the operating system is set to
/// one of them, English otherwise. Existing users keep the language they had (English).
pub fn system_default() -> &'static str {
    Lang::from_code(&os_locale()).code()
}

#[cfg(windows)]
fn os_locale() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    // LOCALE_NAME_MAX_LENGTH
    let mut buf = [0u16; 85];
    let n = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if n > 1 {
        String::from_utf16_lossy(&buf[..(n as usize - 1)])
    } else {
        String::new()
    }
}

#[cfg(target_os = "macos")]
fn os_locale() -> String {
    // Apps started from Finder or the Dock get no LANG; ask the user defaults.
    std::process::Command::new("defaults")
        .args(["read", "-g", "AppleLocale"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(env_locale)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn os_locale() -> String {
    env_locale()
}

#[cfg(not(windows))]
fn env_locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty() && v != "C" && v != "POSIX")
        .unwrap_or_default()
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        x if x == Lang::En as u8 => Lang::En,
        x if x == Lang::Es as u8 => Lang::Es,
        x if x == Lang::Ar as u8 => Lang::Ar,
        x if x == Lang::Id as u8 => Lang::Id,
        _ => Lang::PtBr,
    }
}

fn english(s: &str) -> Option<&'static str> {
    EN_TABLE.iter().find(|(pt, _)| *pt == s).map(|(_, en)| *en)
}

fn spanish(s: &str) -> Option<&'static str> {
    ES_TABLE.iter().find(|(pt, _)| *pt == s).map(|(_, es)| *es)
}

fn arabic(s: &str) -> Option<&'static str> {
    AR_TABLE.iter().find(|(pt, _)| *pt == s).map(|(_, v)| *v)
}

fn indonesian(s: &str) -> Option<&'static str> {
    ID_TABLE.iter().find(|(pt, _)| *pt == s).map(|(_, v)| *v)
}

/// Translate a Portuguese source string into the current language.
pub fn tr(s: &str) -> Cow<'_, str> {
    tr_in(current(), s)
}

pub fn tr_in(lang: Lang, s: &str) -> Cow<'_, str> {
    match lang {
        Lang::PtBr => Cow::Borrowed(s),
        Lang::En => english(s).map_or(Cow::Borrowed(s), Cow::Borrowed),
        Lang::Es => spanish(s)
            .or_else(|| english(s))
            .map_or(Cow::Borrowed(s), Cow::Borrowed),
        Lang::Ar => arabic(s)
            .or_else(|| english(s))
            .map_or(Cow::Borrowed(s), Cow::Borrowed),
        Lang::Id => indonesian(s)
            .or_else(|| english(s))
            .map_or(Cow::Borrowed(s), Cow::Borrowed),
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
    // download folder and app icon
    ("A pasta não existe.", "The folder does not exist."),
    ("Não é possível gravar nessa pasta.", "That folder is not writable."),
    ("Todos os arquivos", "All files"),
    ("Salvar arquivo", "Save file"),
    ("Pasta de downloads", "Download folder"),
    ("Salvar backup do whatRust", "Save whatRust backup"),
    ("Restaurar backup do whatRust", "Restore whatRust backup"),
    ("Este arquivo não é um backup do whatRust.", "This file is not a whatRust backup."),
    ("Ícone do aplicativo", "App icon"),
    ("A imagem tem mais de 5 MB.", "The image is larger than 5 MB."),
    ("O arquivo não é uma imagem PNG.", "The file is not a PNG image."),
    ("A imagem precisa ser quadrada.", "The image must be square."),
    ("A imagem é grande demais (máximo {0} px).", "The image is too large (maximum {0} px)."),
    ("A imagem é pequena demais (mínimo {0} px).", "The image is too small (minimum {0} px)."),
    ("Não foi possível ler a imagem PNG.", "The PNG image could not be read."),
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
    ("Endereço do proxy inválido. Use um nome de host ou um endereço IP, sem http://, usuário ou senha.", "Invalid proxy address. Use a host name or an IP address, without http://, user name or password."),
    ("Tipo de proxy inválido.", "Invalid proxy type."),
    ("A porta do proxy deve estar entre 1 e 65535.", "The proxy port must be between 1 and 65535."),
    ("Endereço do proxy inválido.", "Invalid proxy address."),
    ("O proxy exige macOS 14 (Sonoma) ou superior. Desative o proxy nas configurações.", "The proxy needs macOS 14 (Sonoma) or later. Turn the proxy off in settings."),
    ("O proxy exige macOS 14 (Sonoma) ou superior.", "The proxy needs macOS 14 (Sonoma) or later."),
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
    ("Imagem não colada", "Image not pasted"),
    ("A imagem da área de transferência é grande demais. Salve-a e anexe o arquivo.", "The clipboard image is too large. Save it and attach the file."),
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
    ("O atalho para desfocar não pode ser igual ao atalho de mostrar/ocultar: {0}", "The blur shortcut can't be the same as the show/hide shortcut: {0}"),
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
    ("Desfoque de privacidade…", "Privacy blur…"),
    ("Recarregar conta", "Reload account"),
    ("Sair do whatRust", "Quit whatRust"),
    // updates
    ("Instalador do Windows (NSIS)", "Windows installer (NSIS)"),
    ("Instalador do Windows (MSI)", "Windows installer (MSI)"),
    ("Aplicativo do macOS", "macOS app"),
    ("Compilado do código-fonte", "Built from source"),
    ("Outro tipo de instalação", "Other install type"),
    ("Compilado do código-fonte: atualize o código e compile de novo, ou instale uma versão publicada.", "Built from source: pull the latest code and rebuild, or install a published release."),
    ("Esta instalação não pode ser atualizada pelo whatRust. Baixe a nova versão na página de lançamentos ou use o gerenciador de pacotes.", "whatRust cannot update this installation. Download the new version from the releases page or use your package manager."),
    ("Proxy inválido: {0}", "Invalid proxy: {0}"),
    ("Falha de rede: {0}", "Network error: {0}"),
    ("Resposta inesperada do GitHub: {0}", "Unexpected answer from GitHub: {0}"),
    ("Nenhuma versão publicada encontrada.", "No published release found."),
    ("Download interrompido: {0}", "Download interrupted: {0}"),
    ("Download incompleto ({0} de {1} bytes).", "Download incomplete ({0} of {1} bytes)."),
    ("Esta versão não tem o arquivo SHA256SUMS; nada foi alterado.", "This release has no SHA256SUMS file; nothing was changed."),
    ("O SHA256SUMS não lista {0}; nada foi alterado.", "SHA256SUMS does not list {0}; nothing was changed."),
    ("Esta versão não tem um arquivo para o seu tipo de instalação ({0}).", "This release has no file for your install type ({0})."),
    ("Caminho do AppImage inválido.", "Invalid AppImage path."),
    ("Caminho do aplicativo inválido.", "Invalid app path."),
    ("O endereço do arquivo não pertence às versões oficiais do whatRust.", "The file address is not one of the official whatRust releases."),
    ("Não foi possível criar uma pasta temporária: {0}", "Could not create a temporary folder: {0}"),
    ("{0} está danificado (a soma de verificação não confere); nada foi alterado.", "{0} is damaged (the checksum does not match); nothing was changed."),
    ("Não foi possível substituir {0}: {1}", "Could not replace {0}: {1}"),
    ("Não foi possível instalar a nova versão: {0}{1}", "Could not install the new version: {0}{1}"),
    ("Não foi possível descompactar a atualização.", "Could not unpack the update."),
    ("A atualização não contém um aplicativo.", "The update does not contain an app."),
    ("A atualização não contém um aplicativo válido.", "The update does not contain a valid app."),
    ("A instalação foi cancelada.", "The installation was cancelled."),
    ("O dpkg não conseguiu instalar o pacote ({0}).", "dpkg could not install the package ({0})."),
    ("O Flatpak não conseguiu instalar a atualização ({0}). O pacote verificado está em {1}; instale com: flatpak install {2} --reinstall {1}", "Flatpak could not install the update ({0}). The verified bundle is at {1}; install it with: flatpak install {2} --reinstall {1}"),
    ("O pkexec não foi encontrado. O pacote verificado está em {0}; instale com: sudo dpkg -i {0}", "pkexec was not found. The verified package is at {0}; install it with: sudo dpkg -i {0}"),
    ("Não foi possível iniciar o instalador: {0}", "Could not start the installer: {0}"),
    ("whatRust {0} está disponível", "whatRust {0} is available"),
    ("Abra Configurações → Atualizações para ver as novidades e instalar.", "Open Settings → Updates to see what's new and install it."),
    ("whatRust {0} foi instalado", "whatRust {0} was installed"),
    ("Reinicie o whatRust para usar a nova versão (Configurações → Atualizações).", "Restart whatRust to use the new version (Settings → Updates)."),
    ("whatRust {0} está pronto para instalar", "whatRust {0} is ready to install"),
    ("Abra Configurações → Atualizações e clique em Reiniciar e instalar.", "Open Settings → Updates and click Restart and install."),
    ("Não há atualização instalada para reiniciar.", "There is no installed update to restart into."),
    ("Não foi possível reiniciar o whatRust. Abra-o manualmente.", "Could not restart whatRust. Please open it yourself."),
    ("Atualizar para {0}", "Update to {0}"),
    // window titles
    ("whatRust — Configurações", "whatRust — Settings"),
    ("whatRust — Conta destacada", "whatRust — Detached account"),
];

/// Spanish text for each Portuguese source string (same keys as `EN_TABLE`).
/// A key missing here falls back to English.
static ES_TABLE: &[(&str, &str)] = &[
    ("A pasta não existe.", "La carpeta no existe."),
    ("Não é possível gravar nessa pasta.", "No se puede escribir en esa carpeta."),
    ("Todos os arquivos", "Todos los archivos"),
    ("Salvar arquivo", "Guardar archivo"),
    ("Pasta de downloads", "Carpeta de descargas"),
    ("Salvar backup do whatRust", "Guardar copia de seguridad de whatRust"),
    ("Restaurar backup do whatRust", "Restaurar copia de seguridad de whatRust"),
    ("Este arquivo não é um backup do whatRust.", "Este archivo no es una copia de seguridad de whatRust."),
    ("Ícone do aplicativo", "Icono de la aplicación"),
    ("A imagem tem mais de 5 MB.", "La imagen pesa más de 5 MB."),
    ("O arquivo não é uma imagem PNG.", "El archivo no es una imagen PNG."),
    ("A imagem precisa ser quadrada.", "La imagen debe ser cuadrada."),
    ("A imagem é grande demais (máximo {0} px).", "La imagen es demasiado grande (máximo {0} px)."),
    ("A imagem é pequena demais (mínimo {0} px).", "La imagen es demasiado pequeña (mínimo {0} px)."),
    ("Não foi possível ler a imagem PNG.", "No se pudo leer la imagen PNG."),
    ("Não é possível remover a última conta.", "No se puede eliminar la última cuenta."),
    ("Conta não encontrada: {0}", "Cuenta no encontrada: {0}"),
    ("Conta não encontrada", "Cuenta no encontrada"),
    ("A ordem deve conter todas as contas uma única vez", "El orden debe incluir todas las cuentas una sola vez"),
    ("Conta não está aberta", "La cuenta no está abierta"),
    ("{0}; e não foi possível restaurar a conta: {1}", "{0}; y no se pudo restaurar la cuenta: {1}"),
    ("As senhas não coincidem.", "Las contraseñas no coinciden."),
    ("A senha deve ter pelo menos {0} caracteres.", "La contraseña debe tener al menos {0} caracteres."),
    ("O bloqueio já está ativado.", "El bloqueo ya está activado."),
    ("O bloqueio não está ativado.", "El bloqueo no está activado."),
    ("A senha atual está incorreta.", "La contraseña actual es incorrecta."),
    ("Defina uma senha de bloqueio primeiro.", "Primero define una contraseña de bloqueo."),
    ("A autenticação biométrica não está disponível neste dispositivo.", "La autenticación biométrica no está disponible en este dispositivo."),
    ("Ativar desbloqueio por biometria no whatRust", "Activar el desbloqueo biométrico en whatRust"),
    ("Desbloquear o whatRust", "Desbloquear whatRust"),
    ("Não foi possível confirmar a biometria.", "No se pudo confirmar la biometría."),
    ("O desbloqueio por biometria não está ativado.", "El desbloqueo biométrico no está activado."),
    ("O tempo para confirmar a biometria terminou.", "Se agotó el tiempo para confirmar la biometría."),
    ("A redefinição precisa ser confirmada.", "Hay que confirmar el restablecimiento."),
    ("Endereço do proxy inválido. Use um nome de host ou um endereço IP, sem http://, usuário ou senha.", "Dirección de proxy no válida. Usa un nombre de host o una dirección IP, sin http://, usuario ni contraseña."),
    ("Tipo de proxy inválido.", "Tipo de proxy no válido."),
    ("A porta do proxy deve estar entre 1 e 65535.", "El puerto del proxy debe estar entre 1 y 65535."),
    ("Endereço do proxy inválido.", "Dirección de proxy no válida."),
    ("O proxy exige macOS 14 (Sonoma) ou superior. Desative o proxy nas configurações.", "El proxy requiere macOS 14 (Sonoma) o posterior. Desactiva el proxy en la configuración."),
    ("O proxy exige macOS 14 (Sonoma) ou superior.", "El proxy requiere macOS 14 (Sonoma) o posterior."),
    ("Não foi possível concluir a redefinição. Feche outras instâncias do whatRust e tente novamente.", "No se pudo completar el restablecimiento. Cierra las otras instancias de whatRust e inténtalo de nuevo."),
    ("whatRust — Bloqueado", "whatRust — Bloqueado"),
    ("Download não encontrado", "Descarga no encontrada"),
    ("Download concluído", "Descarga completa"),
    ("Falha no download", "Error en la descarga"),
    ("{0} — salvo no local escolhido.", "{0} — guardado en la ubicación elegida."),
    ("Arquivo salvo no local escolhido.", "Archivo guardado en la ubicación elegida."),
    ("Não foi possível baixar o arquivo. Tente novamente.", "No se pudo descargar el archivo. Inténtalo de nuevo."),
    ("Não foi possível abrir o link", "No se pudo abrir el enlace"),
    ("Não foi possível iniciar o navegador. Copie o link e abra-o no navegador.", "No se pudo iniciar el navegador. Copia el enlace y ábrelo en el navegador."),
    ("Não foi possível abrir o arquivo", "No se pudo abrir el archivo"),
    ("Abra o arquivo pela pasta em que ele foi salvo.", "Abre el archivo desde la carpeta donde se guardó."),
    ("Nenhum arquivo foi anexado", "No se adjuntó ningún archivo"),
    ("Não é possível arrastar esse item diretamente. Arraste o arquivo pelo Explorador de Arquivos ou use o botão de anexo (+).", "No se puede arrastrar este elemento directamente. Arrastra el archivo desde el Explorador de archivos o usa el botón de adjuntar (+)."),
    ("Arquivos não anexados", "Archivos no adjuntados"),
    ("Não foi possível processar os arquivos. Tente novamente.", "No se pudieron procesar los archivos. Inténtalo de nuevo."),
    ("Não foi possível enviar os arquivos ao WhatsApp. Tente novamente.", "No se pudieron enviar los archivos a WhatsApp. Inténtalo de nuevo."),
    ("O WhatsApp ainda não estava pronto para receber os arquivos. Tente novamente.", "WhatsApp todavía no estaba listo para recibir los archivos. Inténtalo de nuevo."),
    ("Imagem não colada", "Imagen no pegada"),
    ("A imagem da área de transferência é grande demais. Salve-a e anexe o arquivo.", "La imagen del portapapeles es demasiado grande. Guárdala y adjunta el archivo."),
    ("Alguns arquivos não foram anexados", "Algunos archivos no se adjuntaron"),
    ("arquivo", "archivo"),
    ("arquivos", "archivos"),
    ("{0} {1} acima do limite de 500 MB", "{0} {1} por encima del límite de 500 MB"),
    ("{0} {1} acima do total de 500 MB por envio", "{0} {1} por encima del total de 500 MB por envío"),
    ("{0} {1} acima do limite de 200 arquivos", "{0} {1} por encima del límite de 200 archivos"),
    ("pastas ou arquivos especiais: {0}", "carpetas o archivos especiales: {0}"),
    ("{0} {1} sem permissão de leitura", "{0} {1} sin permiso de lectura"),
    ("{0} {1} alterados durante a leitura", "{0} {1} modificados durante la lectura"),
    ("Não anexados: {0}.", "No adjuntados: {0}."),
    ("Nova mensagem", "Mensaje nuevo"),
    ("Você tem uma nova mensagem.", "Tienes un mensaje nuevo."),
    ("Você tem novas mensagens em {0} conversas.", "Tienes mensajes nuevos en {0} chats."),
    ("Você recebeu uma mensagem no WhatsApp.", "Recibiste un mensaje en WhatsApp."),
    ("Teste do whatRust", "Prueba de whatRust"),
    ("Se você está vendo este aviso, o Windows exibiu o teste.", "Si ves este aviso, Windows mostró la prueba."),
    ("Ative as notificações antes do teste.", "Activa las notificaciones antes de la prueba."),
    ("Teste solicitado. Confira o banner e a central do Windows; aceitação não confirma exibição.", "Prueba solicitada. Revisa el banner y el centro de notificaciones de Windows; que se acepte no confirma que se haya mostrado."),
    ("Não foi possível abrir as configurações do Windows", "No se pudo abrir la configuración de Windows"),
    ("Disponível no Windows", "Disponible en Windows"),
    ("Não foi possível atualizar a inicialização automática: {0}", "No se pudo actualizar el inicio automático: {0}"),
    ("Não foi possível registrar o atalho (ele pode estar em uso): {0}", "No se pudo registrar el atajo (puede estar en uso): {0}"),
    ("O atalho para desfocar não pode ser igual ao atalho de mostrar/ocultar: {0}", "El atajo para desenfocar no puede ser igual al atajo para mostrar u ocultar: {0}"),
    ("Contas…", "Cuentas…"),
    ("Contas", "Cuentas"),
    ("Configurações", "Configuración"),
    ("Recarregar", "Recargar"),
    ("Sair", "Salir"),
    ("Bloquear agora", "Bloquear ahora"),
    ("Bloquear", "Bloquear"),
    ("{0} conversas não lidas — WhatsApp", "{0} chats sin leer — WhatsApp"),
    ("Configurações do whatRust", "Configuración de whatRust"),
    ("Voltar ao WhatsApp", "Volver a WhatsApp"),
    ("Desfoque de privacidade…", "Desenfoque de privacidad…"),
    ("Recarregar conta", "Recargar cuenta"),
    ("Sair do whatRust", "Salir de whatRust"),
    ("Instalador do Windows (NSIS)", "Instalador de Windows (NSIS)"),
    ("Instalador do Windows (MSI)", "Instalador de Windows (MSI)"),
    ("Aplicativo do macOS", "Aplicación de macOS"),
    ("Compilado do código-fonte", "Compilado desde el código fuente"),
    ("Outro tipo de instalação", "Otro tipo de instalación"),
    ("Compilado do código-fonte: atualize o código e compile de novo, ou instale uma versão publicada.", "Compilado desde el código fuente: actualiza el código y vuelve a compilar, o instala una versión publicada."),
    ("Esta instalação não pode ser atualizada pelo whatRust. Baixe a nova versão na página de lançamentos ou use o gerenciador de pacotes.", "whatRust no puede actualizar esta instalación. Descarga la nueva versión desde la página de versiones o usa tu gestor de paquetes."),
    ("Proxy inválido: {0}", "Proxy no válido: {0}"),
    ("Falha de rede: {0}", "Error de red: {0}"),
    ("Resposta inesperada do GitHub: {0}", "Respuesta inesperada de GitHub: {0}"),
    ("Nenhuma versão publicada encontrada.", "No se encontró ninguna versión publicada."),
    ("Download interrompido: {0}", "Descarga interrumpida: {0}"),
    ("Download incompleto ({0} de {1} bytes).", "Descarga incompleta ({0} de {1} bytes)."),
    ("Esta versão não tem o arquivo SHA256SUMS; nada foi alterado.", "Esta versión no tiene el archivo SHA256SUMS; no se cambió nada."),
    ("O SHA256SUMS não lista {0}; nada foi alterado.", "SHA256SUMS no incluye {0}; no se cambió nada."),
    ("Esta versão não tem um arquivo para o seu tipo de instalação ({0}).", "Esta versión no tiene un archivo para tu tipo de instalación ({0})."),
    ("Caminho do AppImage inválido.", "Ruta de AppImage no válida."),
    ("Caminho do aplicativo inválido.", "Ruta de la aplicación no válida."),
    ("O endereço do arquivo não pertence às versões oficiais do whatRust.", "La dirección del archivo no pertenece a las versiones oficiales de whatRust."),
    ("Não foi possível criar uma pasta temporária: {0}", "No se pudo crear una carpeta temporal: {0}"),
    ("{0} está danificado (a soma de verificação não confere); nada foi alterado.", "{0} está dañado (la suma de verificación no coincide); no se cambió nada."),
    ("Não foi possível substituir {0}: {1}", "No se pudo reemplazar {0}: {1}"),
    ("Não foi possível instalar a nova versão: {0}{1}", "No se pudo instalar la nueva versión: {0}{1}"),
    ("Não foi possível descompactar a atualização.", "No se pudo descomprimir la actualización."),
    ("A atualização não contém um aplicativo.", "La actualización no contiene una aplicación."),
    ("A atualização não contém um aplicativo válido.", "La actualización no contiene una aplicación válida."),
    ("A instalação foi cancelada.", "Se canceló la instalación."),
    ("O dpkg não conseguiu instalar o pacote ({0}).", "dpkg no pudo instalar el paquete ({0})."),
    ("O Flatpak não conseguiu instalar a atualização ({0}). O pacote verificado está em {1}; instale com: flatpak install {2} --reinstall {1}", "Flatpak no pudo instalar la actualización ({0}). El paquete verificado está en {1}; instálalo con: flatpak install {2} --reinstall {1}"),
    ("O pkexec não foi encontrado. O pacote verificado está em {0}; instale com: sudo dpkg -i {0}", "No se encontró pkexec. El paquete verificado está en {0}; instálalo con: sudo dpkg -i {0}"),
    ("Não foi possível iniciar o instalador: {0}", "No se pudo iniciar el instalador: {0}"),
    ("whatRust {0} está disponível", "whatRust {0} está disponible"),
    ("Abra Configurações → Atualizações para ver as novidades e instalar.", "Abre Configuración → Actualizaciones para ver las novedades e instalarla."),
    ("whatRust {0} foi instalado", "whatRust {0} se instaló"),
    ("Reinicie o whatRust para usar a nova versão (Configurações → Atualizações).", "Reinicia whatRust para usar la nueva versión (Configuración → Actualizaciones)."),
    ("whatRust {0} está pronto para instalar", "whatRust {0} está listo para instalar"),
    ("Abra Configurações → Atualizações e clique em Reiniciar e instalar.", "Abre Configuración → Actualizaciones y haz clic en Reiniciar e instalar."),
    ("Não há atualização instalada para reiniciar.", "No hay ninguna actualización instalada para reiniciar."),
    ("Não foi possível reiniciar o whatRust. Abra-o manualmente.", "No se pudo reiniciar whatRust. Ábrelo manualmente."),
    ("Atualizar para {0}", "Actualizar a {0}"),
    ("whatRust — Configurações", "whatRust — Configuración"),
    ("whatRust — Conta destacada", "whatRust — Cuenta separada"),
];

/// Arabic text for each Portuguese source string. A key missing here falls back to English.
static AR_TABLE: &[(&str, &str)] = &[
    ("A pasta não existe.", "المجلد غير موجود."),
    ("Não é possível gravar nessa pasta.", "لا يمكن الكتابة في هذا المجلد."),
    ("Todos os arquivos", "كل الملفات"),
    ("Salvar arquivo", "حفظ الملف"),
    ("Pasta de downloads", "مجلد التنزيلات"),
    ("Salvar backup do whatRust", "حفظ نسخة whatRust الاحتياطية"),
    ("Restaurar backup do whatRust", "استعادة نسخة whatRust الاحتياطية"),
    ("Este arquivo não é um backup do whatRust.", "هذا الملف ليس نسخة احتياطية من whatRust."),
    ("Ícone do aplicativo", "أيقونة التطبيق"),
    ("A imagem tem mais de 5 MB.", "الصورة أكبر من 5 ميغابايت."),
    ("O arquivo não é uma imagem PNG.", "الملف ليس صورة PNG."),
    ("A imagem precisa ser quadrada.", "يجب أن تكون الصورة مربعة."),
    ("A imagem é grande demais (máximo {0} px).", "الصورة كبيرة جدًا (الحد الأقصى {0} بكسل)."),
    ("A imagem é pequena demais (mínimo {0} px).", "الصورة صغيرة جدًا (الحد الأدنى {0} بكسل)."),
    ("Não foi possível ler a imagem PNG.", "تعذّرت قراءة صورة PNG."),
    ("Não é possível remover a última conta.", "لا يمكن إزالة آخر حساب."),
    ("Conta não encontrada: {0}", "الحساب غير موجود: {0}"),
    ("Conta não encontrada", "الحساب غير موجود"),
    ("A ordem deve conter todas as contas uma única vez", "يجب أن يتضمن الترتيب كل حساب مرة واحدة بالضبط"),
    ("Conta não está aberta", "الحساب غير مفتوح"),
    ("{0}; e não foi possível restaurar a conta: {1}", "{0}؛ وتعذّرت استعادة الحساب: {1}"),
    ("As senhas não coincidem.", "كلمتا المرور غير متطابقتين."),
    ("A senha deve ter pelo menos {0} caracteres.", "يجب ألا تقل كلمة المرور عن {0} أحرف."),
    ("O bloqueio já está ativado.", "القفل مفعّل بالفعل."),
    ("O bloqueio não está ativado.", "القفل غير مفعّل."),
    ("A senha atual está incorreta.", "كلمة المرور الحالية غير صحيحة."),
    ("Defina uma senha de bloqueio primeiro.", "عيّن كلمة مرور للقفل أولًا."),
    ("A autenticação biométrica não está disponível neste dispositivo.", "المصادقة بالبصمة الحيوية غير متاحة على هذا الجهاز."),
    ("Ativar desbloqueio por biometria no whatRust", "تفعيل فتح القفل بالبصمة الحيوية في whatRust"),
    ("Desbloquear o whatRust", "فتح قفل whatRust"),
    ("Não foi possível confirmar a biometria.", "تعذّر تأكيد البصمة الحيوية."),
    ("O desbloqueio por biometria não está ativado.", "فتح القفل بالبصمة الحيوية غير مفعّل."),
    ("O tempo para confirmar a biometria terminou.", "انتهت مهلة تأكيد البصمة الحيوية."),
    ("A redefinição precisa ser confirmada.", "يجب تأكيد إعادة التعيين."),
    ("Endereço do proxy inválido. Use um nome de host ou um endereço IP, sem http://, usuário ou senha.", "عنوان البروكسي غير صالح. استخدم اسم مضيف أو عنوان IP، دون http:// أو اسم مستخدم أو كلمة مرور."),
    ("Tipo de proxy inválido.", "نوع البروكسي غير صالح."),
    ("A porta do proxy deve estar entre 1 e 65535.", "يجب أن يكون منفذ البروكسي بين 1 و65535."),
    ("Endereço do proxy inválido.", "عنوان البروكسي غير صالح."),
    ("O proxy exige macOS 14 (Sonoma) ou superior. Desative o proxy nas configurações.", "يتطلب البروكسي macOS 14 (Sonoma) أو أحدث. أوقف البروكسي في الإعدادات."),
    ("O proxy exige macOS 14 (Sonoma) ou superior.", "يتطلب البروكسي macOS 14 (Sonoma) أو أحدث."),
    ("Não foi possível concluir a redefinição. Feche outras instâncias do whatRust e tente novamente.", "تعذّر إكمال إعادة التعيين. أغلق النسخ الأخرى من whatRust وحاول مرة أخرى."),
    ("whatRust — Bloqueado", "whatRust — مقفل"),
    ("Download não encontrado", "التنزيل غير موجود"),
    ("Download concluído", "اكتمل التنزيل"),
    ("Falha no download", "فشل التنزيل"),
    ("{0} — salvo no local escolhido.", "{0} — حُفظ في الموقع المختار."),
    ("Arquivo salvo no local escolhido.", "حُفظ الملف في الموقع المختار."),
    ("Não foi possível baixar o arquivo. Tente novamente.", "تعذّر تنزيل الملف. حاول مرة أخرى."),
    ("Não foi possível abrir o link", "تعذّر فتح الرابط"),
    ("Não foi possível iniciar o navegador. Copie o link e abra-o no navegador.", "تعذّر تشغيل المتصفح. انسخ الرابط وافتحه في متصفحك."),
    ("Não foi possível abrir o arquivo", "تعذّر فتح الملف"),
    ("Abra o arquivo pela pasta em que ele foi salvo.", "افتح الملف من المجلد الذي حُفظ فيه."),
    ("Nenhum arquivo foi anexado", "لم يُرفق أي ملف"),
    ("Não é possível arrastar esse item diretamente. Arraste o arquivo pelo Explorador de Arquivos ou use o botão de anexo (+).", "لا يمكن سحب هذا العنصر مباشرة. اسحب الملف من مستكشف الملفات أو استخدم زر الإرفاق (+)."),
    ("Arquivos não anexados", "لم تُرفق الملفات"),
    ("Não foi possível processar os arquivos. Tente novamente.", "تعذّرت معالجة الملفات. حاول مرة أخرى."),
    ("Não foi possível enviar os arquivos ao WhatsApp. Tente novamente.", "تعذّر إرسال الملفات إلى WhatsApp. حاول مرة أخرى."),
    ("O WhatsApp ainda não estava pronto para receber os arquivos. Tente novamente.", "لم يكن WhatsApp جاهزًا لاستلام الملفات. حاول مرة أخرى."),
    ("Imagem não colada", "لم تُلصق الصورة"),
    ("A imagem da área de transferência é grande demais. Salve-a e anexe o arquivo.", "صورة الحافظة كبيرة جدًا. احفظها وأرفق الملف."),
    ("Alguns arquivos não foram anexados", "لم تُرفق بعض الملفات"),
    ("arquivo", "ملف"),
    ("arquivos", "ملفات"),
    ("{0} {1} acima do limite de 500 MB", "{0} {1} تتجاوز حد 500 ميغابايت"),
    ("{0} {1} acima do total de 500 MB por envio", "{0} {1} تتجاوز إجمالي 500 ميغابايت لكل رفع"),
    ("{0} {1} acima do limite de 200 arquivos", "{0} {1} تتجاوز حد 200 ملف"),
    ("pastas ou arquivos especiais: {0}", "مجلدات أو ملفات خاصة: {0}"),
    ("{0} {1} sem permissão de leitura", "{0} {1} بلا إذن قراءة"),
    ("{0} {1} alterados durante a leitura", "{0} {1} تغيّرت أثناء القراءة"),
    ("Não anexados: {0}.", "لم يُرفق: {0}."),
    ("Nova mensagem", "رسالة جديدة"),
    ("Você tem uma nova mensagem.", "لديك رسالة جديدة."),
    ("Você tem novas mensagens em {0} conversas.", "لديك رسائل جديدة في {0} محادثات."),
    ("Você recebeu uma mensagem no WhatsApp.", "وصلتك رسالة على WhatsApp."),
    ("Teste do whatRust", "اختبار whatRust"),
    ("Se você está vendo este aviso, o Windows exibiu o teste.", "إذا كنت ترى هذا الإشعار، فقد عرض Windows الاختبار."),
    ("Ative as notificações antes do teste.", "فعّل الإشعارات قبل الاختبار."),
    ("Teste solicitado. Confira o banner e a central do Windows; aceitação não confirma exibição.", "طُلب الاختبار. تحقق من الشعار ومركز إشعارات Windows؛ قبول الطلب لا يؤكد أنه عُرض."),
    ("Não foi possível abrir as configurações do Windows", "تعذّر فتح إعدادات Windows"),
    ("Disponível no Windows", "متاح على Windows"),
    ("Não foi possível atualizar a inicialização automática: {0}", "تعذّر تحديث التشغيل التلقائي: {0}"),
    ("Não foi possível registrar o atalho (ele pode estar em uso): {0}", "تعذّر تسجيل الاختصار (قد يكون قيد الاستخدام): {0}"),
    ("O atalho para desfocar não pode ser igual ao atalho de mostrar/ocultar: {0}", "لا يمكن أن يكون اختصار التمويه هو نفسه اختصار الإظهار/الإخفاء: {0}"),
    ("Contas…", "الحسابات…"),
    ("Contas", "الحسابات"),
    ("Configurações", "الإعدادات"),
    ("Recarregar", "إعادة تحميل"),
    ("Sair", "إنهاء"),
    ("Bloquear agora", "قفل الآن"),
    ("Bloquear", "قفل"),
    ("{0} conversas não lidas — WhatsApp", "{0} محادثات غير مقروءة — WhatsApp"),
    ("Configurações do whatRust", "إعدادات whatRust"),
    ("Voltar ao WhatsApp", "العودة إلى WhatsApp"),
    ("Desfoque de privacidade…", "تمويه الخصوصية…"),
    ("Recarregar conta", "إعادة تحميل الحساب"),
    ("Sair do whatRust", "إنهاء whatRust"),
    ("Instalador do Windows (NSIS)", "مثبّت Windows (NSIS)"),
    ("Instalador do Windows (MSI)", "مثبّت Windows (MSI)"),
    ("Aplicativo do macOS", "تطبيق macOS"),
    ("Compilado do código-fonte", "مبني من المصدر"),
    ("Outro tipo de instalação", "نوع تثبيت آخر"),
    ("Compilado do código-fonte: atualize o código e compile de novo, ou instale uma versão publicada.", "مبني من المصدر: اسحب أحدث شيفرة وأعد البناء، أو ثبّت إصدارًا منشورًا."),
    ("Esta instalação não pode ser atualizada pelo whatRust. Baixe a nova versão na página de lançamentos ou use o gerenciador de pacotes.", "لا يستطيع whatRust تحديث هذا التثبيت. نزّل الإصدار الجديد من صفحة الإصدارات أو استخدم مدير الحزم."),
    ("Proxy inválido: {0}", "بروكسي غير صالح: {0}"),
    ("Falha de rede: {0}", "خطأ في الشبكة: {0}"),
    ("Resposta inesperada do GitHub: {0}", "ردّ غير متوقع من GitHub: {0}"),
    ("Nenhuma versão publicada encontrada.", "لم يُعثر على إصدار منشور."),
    ("Download interrompido: {0}", "انقطع التنزيل: {0}"),
    ("Download incompleto ({0} de {1} bytes).", "التنزيل غير مكتمل ({0} من {1} بايت)."),
    ("Esta versão não tem o arquivo SHA256SUMS; nada foi alterado.", "لا يحتوي هذا الإصدار على ملف SHA256SUMS؛ لم يتغير شيء."),
    ("O SHA256SUMS não lista {0}; nada foi alterado.", "لا يذكر SHA256SUMS الملف {0}؛ لم يتغير شيء."),
    ("Esta versão não tem um arquivo para o seu tipo de instalação ({0}).", "لا يحتوي هذا الإصدار على ملف لنوع التثبيت لديك ({0})."),
    ("Caminho do AppImage inválido.", "مسار AppImage غير صالح."),
    ("Caminho do aplicativo inválido.", "مسار التطبيق غير صالح."),
    ("O endereço do arquivo não pertence às versões oficiais do whatRust.", "عنوان الملف ليس من إصدارات whatRust الرسمية."),
    ("Não foi possível criar uma pasta temporária: {0}", "تعذّر إنشاء مجلد مؤقت: {0}"),
    ("{0} está danificado (a soma de verificação não confere); nada foi alterado.", "{0} تالف (المجموع الاختباري غير مطابق)؛ لم يتغير شيء."),
    ("Não foi possível substituir {0}: {1}", "تعذّر استبدال {0}: {1}"),
    ("Não foi possível instalar a nova versão: {0}{1}", "تعذّر تثبيت الإصدار الجديد: {0}{1}"),
    ("Não foi possível descompactar a atualização.", "تعذّر فك ضغط التحديث."),
    ("A atualização não contém um aplicativo.", "لا يحتوي التحديث على تطبيق."),
    ("A atualização não contém um aplicativo válido.", "لا يحتوي التحديث على تطبيق صالح."),
    ("A instalação foi cancelada.", "أُلغي التثبيت."),
    ("O dpkg não conseguiu instalar o pacote ({0}).", "تعذّر على dpkg تثبيت الحزمة ({0})."),
    ("O Flatpak não conseguiu instalar a atualização ({0}). O pacote verificado está em {1}; instale com: flatpak install {2} --reinstall {1}", "تعذّر على Flatpak تثبيت التحديث ({0}). الحزمة الموثّقة في {1}؛ ثبّتها بالأمر: flatpak install {2} --reinstall {1}"),
    ("O pkexec não foi encontrado. O pacote verificado está em {0}; instale com: sudo dpkg -i {0}", "لم يُعثر على pkexec. الحزمة الموثّقة في {0}؛ ثبّتها بالأمر: sudo dpkg -i {0}"),
    ("Não foi possível iniciar o instalador: {0}", "تعذّر تشغيل المثبّت: {0}"),
    ("whatRust {0} está disponível", "whatRust {0} متاح"),
    ("Abra Configurações → Atualizações para ver as novidades e instalar.", "افتح الإعدادات ← التحديثات لمعرفة الجديد وتثبيته."),
    ("whatRust {0} foi instalado", "تم تثبيت whatRust {0}"),
    ("Reinicie o whatRust para usar a nova versão (Configurações → Atualizações).", "أعد تشغيل whatRust لاستخدام الإصدار الجديد (الإعدادات ← التحديثات)."),
    ("whatRust {0} está pronto para instalar", "whatRust {0} جاهز للتثبيت"),
    ("Abra Configurações → Atualizações e clique em Reiniciar e instalar.", "افتح الإعدادات ← التحديثات وانقر «إعادة التشغيل والتثبيت»."),
    ("Não há atualização instalada para reiniciar.", "لا يوجد تحديث مثبّت لإعادة التشغيل إليه."),
    ("Não foi possível reiniciar o whatRust. Abra-o manualmente.", "تعذّرت إعادة تشغيل whatRust. يرجى فتحه بنفسك."),
    ("Atualizar para {0}", "التحديث إلى {0}"),
    ("whatRust — Configurações", "whatRust — الإعدادات"),
    ("whatRust — Conta destacada", "whatRust — حساب منفصل"),
];

/// Indonesian text for each Portuguese source string. A key missing here falls back to English.
static ID_TABLE: &[(&str, &str)] = &[
    ("A pasta não existe.", "Folder tidak ada."),
    ("Não é possível gravar nessa pasta.", "Folder itu tidak dapat ditulisi."),
    ("Todos os arquivos", "Semua berkas"),
    ("Salvar arquivo", "Simpan berkas"),
    ("Pasta de downloads", "Folder unduhan"),
    ("Salvar backup do whatRust", "Simpan cadangan whatRust"),
    ("Restaurar backup do whatRust", "Pulihkan cadangan whatRust"),
    ("Este arquivo não é um backup do whatRust.", "Berkas ini bukan cadangan whatRust."),
    ("Ícone do aplicativo", "Ikon aplikasi"),
    ("A imagem tem mais de 5 MB.", "Gambar lebih besar dari 5 MB."),
    ("O arquivo não é uma imagem PNG.", "Berkas bukan gambar PNG."),
    ("A imagem precisa ser quadrada.", "Gambar harus persegi."),
    ("A imagem é grande demais (máximo {0} px).", "Gambar terlalu besar (maksimal {0} px)."),
    ("A imagem é pequena demais (mínimo {0} px).", "Gambar terlalu kecil (minimal {0} px)."),
    ("Não foi possível ler a imagem PNG.", "Gambar PNG tidak dapat dibaca."),
    ("Não é possível remover a última conta.", "Akun terakhir tidak dapat dihapus."),
    ("Conta não encontrada: {0}", "Akun tidak ditemukan: {0}"),
    ("Conta não encontrada", "Akun tidak ditemukan"),
    ("A ordem deve conter todas as contas uma única vez", "Urutan harus memuat setiap akun tepat satu kali"),
    ("Conta não está aberta", "Akun tidak terbuka"),
    ("{0}; e não foi possível restaurar a conta: {1}", "{0}; dan akun tidak dapat dipulihkan: {1}"),
    ("As senhas não coincidem.", "Kata sandi tidak cocok."),
    ("A senha deve ter pelo menos {0} caracteres.", "Kata sandi minimal {0} karakter."),
    ("O bloqueio já está ativado.", "Kunci sudah aktif."),
    ("O bloqueio não está ativado.", "Kunci tidak aktif."),
    ("A senha atual está incorreta.", "Kata sandi saat ini salah."),
    ("Defina uma senha de bloqueio primeiro.", "Setel kata sandi kunci terlebih dahulu."),
    ("A autenticação biométrica não está disponível neste dispositivo.", "Autentikasi biometrik tidak tersedia di perangkat ini."),
    ("Ativar desbloqueio por biometria no whatRust", "Aktifkan buka kunci biometrik di whatRust"),
    ("Desbloquear o whatRust", "Buka kunci whatRust"),
    ("Não foi possível confirmar a biometria.", "Biometrik tidak dapat dikonfirmasi."),
    ("O desbloqueio por biometria não está ativado.", "Buka kunci biometrik tidak aktif."),
    ("O tempo para confirmar a biometria terminou.", "Waktu untuk mengonfirmasi biometrik habis."),
    ("A redefinição precisa ser confirmada.", "Penyetelan ulang harus dikonfirmasi."),
    ("Endereço do proxy inválido. Use um nome de host ou um endereço IP, sem http://, usuário ou senha.", "Alamat proxy tidak valid. Gunakan nama host atau alamat IP, tanpa http://, nama pengguna, atau kata sandi."),
    ("Tipo de proxy inválido.", "Jenis proxy tidak valid."),
    ("A porta do proxy deve estar entre 1 e 65535.", "Port proxy harus antara 1 dan 65535."),
    ("Endereço do proxy inválido.", "Alamat proxy tidak valid."),
    ("O proxy exige macOS 14 (Sonoma) ou superior. Desative o proxy nas configurações.", "Proxy memerlukan macOS 14 (Sonoma) atau lebih baru. Matikan proxy di pengaturan."),
    ("O proxy exige macOS 14 (Sonoma) ou superior.", "Proxy memerlukan macOS 14 (Sonoma) atau lebih baru."),
    ("Não foi possível concluir a redefinição. Feche outras instâncias do whatRust e tente novamente.", "Penyetelan ulang tidak dapat diselesaikan. Tutup instance whatRust lainnya dan coba lagi."),
    ("whatRust — Bloqueado", "whatRust — Terkunci"),
    ("Download não encontrado", "Unduhan tidak ditemukan"),
    ("Download concluído", "Unduhan selesai"),
    ("Falha no download", "Unduhan gagal"),
    ("{0} — salvo no local escolhido.", "{0} — disimpan di lokasi yang dipilih."),
    ("Arquivo salvo no local escolhido.", "Berkas disimpan di lokasi yang dipilih."),
    ("Não foi possível baixar o arquivo. Tente novamente.", "Berkas tidak dapat diunduh. Coba lagi."),
    ("Não foi possível abrir o link", "Tidak dapat membuka tautan"),
    ("Não foi possível iniciar o navegador. Copie o link e abra-o no navegador.", "Tidak dapat memulai browser. Salin tautan dan buka di browser Anda."),
    ("Não foi possível abrir o arquivo", "Tidak dapat membuka berkas"),
    ("Abra o arquivo pela pasta em que ele foi salvo.", "Buka berkas dari folder tempat berkas disimpan."),
    ("Nenhum arquivo foi anexado", "Tidak ada berkas yang dilampirkan"),
    ("Não é possível arrastar esse item diretamente. Arraste o arquivo pelo Explorador de Arquivos ou use o botão de anexo (+).", "Item ini tidak dapat diseret langsung. Seret berkas dari File Explorer atau gunakan tombol lampir (+)."),
    ("Arquivos não anexados", "Berkas tidak dilampirkan"),
    ("Não foi possível processar os arquivos. Tente novamente.", "Berkas tidak dapat diproses. Coba lagi."),
    ("Não foi possível enviar os arquivos ao WhatsApp. Tente novamente.", "Berkas tidak dapat dikirim ke WhatsApp. Coba lagi."),
    ("O WhatsApp ainda não estava pronto para receber os arquivos. Tente novamente.", "WhatsApp belum siap menerima berkas. Coba lagi."),
    ("Imagem não colada", "Gambar tidak ditempel"),
    ("A imagem da área de transferência é grande demais. Salve-a e anexe o arquivo.", "Gambar di clipboard terlalu besar. Simpan dan lampirkan berkasnya."),
    ("Alguns arquivos não foram anexados", "Sebagian berkas tidak dilampirkan"),
    ("arquivo", "berkas"),
    ("arquivos", "berkas"),
    ("{0} {1} acima do limite de 500 MB", "{0} {1} melebihi batas 500 MB"),
    ("{0} {1} acima do total de 500 MB por envio", "{0} {1} melebihi total 500 MB per unggahan"),
    ("{0} {1} acima do limite de 200 arquivos", "{0} {1} melebihi batas 200 berkas"),
    ("pastas ou arquivos especiais: {0}", "folder atau berkas khusus: {0}"),
    ("{0} {1} sem permissão de leitura", "{0} {1} tanpa izin baca"),
    ("{0} {1} alterados durante a leitura", "{0} {1} berubah saat dibaca"),
    ("Não anexados: {0}.", "Tidak dilampirkan: {0}."),
    ("Nova mensagem", "Pesan baru"),
    ("Você tem uma nova mensagem.", "Anda punya pesan baru."),
    ("Você tem novas mensagens em {0} conversas.", "Anda punya pesan baru di {0} obrolan."),
    ("Você recebeu uma mensagem no WhatsApp.", "Anda menerima pesan di WhatsApp."),
    ("Teste do whatRust", "Tes whatRust"),
    ("Se você está vendo este aviso, o Windows exibiu o teste.", "Jika Anda melihat pemberitahuan ini, Windows menampilkan tes tersebut."),
    ("Ative as notificações antes do teste.", "Aktifkan notifikasi sebelum tes."),
    ("Teste solicitado. Confira o banner e a central do Windows; aceitação não confirma exibição.", "Tes diminta. Periksa banner dan pusat notifikasi Windows; diterimanya permintaan tidak memastikan notifikasi ditampilkan."),
    ("Não foi possível abrir as configurações do Windows", "Tidak dapat membuka pengaturan Windows"),
    ("Disponível no Windows", "Tersedia di Windows"),
    ("Não foi possível atualizar a inicialização automática: {0}", "Tidak dapat memperbarui mulai otomatis: {0}"),
    ("Não foi possível registrar o atalho (ele pode estar em uso): {0}", "Tidak dapat mendaftarkan pintasan (mungkin sedang dipakai): {0}"),
    ("O atalho para desfocar não pode ser igual ao atalho de mostrar/ocultar: {0}", "Pintasan buram tidak boleh sama dengan pintasan tampil/sembunyi: {0}"),
    ("Contas…", "Akun…"),
    ("Contas", "Akun"),
    ("Configurações", "Pengaturan"),
    ("Recarregar", "Muat ulang"),
    ("Sair", "Keluar"),
    ("Bloquear agora", "Kunci sekarang"),
    ("Bloquear", "Kunci"),
    ("{0} conversas não lidas — WhatsApp", "{0} obrolan belum dibaca — WhatsApp"),
    ("Configurações do whatRust", "Pengaturan whatRust"),
    ("Voltar ao WhatsApp", "Kembali ke WhatsApp"),
    ("Desfoque de privacidade…", "Buram privasi…"),
    ("Recarregar conta", "Muat ulang akun"),
    ("Sair do whatRust", "Keluar dari whatRust"),
    ("Instalador do Windows (NSIS)", "Installer Windows (NSIS)"),
    ("Instalador do Windows (MSI)", "Installer Windows (MSI)"),
    ("Aplicativo do macOS", "Aplikasi macOS"),
    ("Compilado do código-fonte", "Dibangun dari sumber"),
    ("Outro tipo de instalação", "Jenis instalasi lain"),
    ("Compilado do código-fonte: atualize o código e compile de novo, ou instale uma versão publicada.", "Dibangun dari sumber: tarik kode terbaru dan bangun ulang, atau pasang rilis yang dipublikasikan."),
    ("Esta instalação não pode ser atualizada pelo whatRust. Baixe a nova versão na página de lançamentos ou use o gerenciador de pacotes.", "whatRust tidak dapat memperbarui instalasi ini. Unduh versi baru dari halaman rilis atau gunakan pengelola paket Anda."),
    ("Proxy inválido: {0}", "Proxy tidak valid: {0}"),
    ("Falha de rede: {0}", "Kesalahan jaringan: {0}"),
    ("Resposta inesperada do GitHub: {0}", "Jawaban tak terduga dari GitHub: {0}"),
    ("Nenhuma versão publicada encontrada.", "Tidak ada rilis yang dipublikasikan."),
    ("Download interrompido: {0}", "Unduhan terputus: {0}"),
    ("Download incompleto ({0} de {1} bytes).", "Unduhan tidak lengkap ({0} dari {1} byte)."),
    ("Esta versão não tem o arquivo SHA256SUMS; nada foi alterado.", "Rilis ini tidak memiliki berkas SHA256SUMS; tidak ada yang diubah."),
    ("O SHA256SUMS não lista {0}; nada foi alterado.", "SHA256SUMS tidak mencantumkan {0}; tidak ada yang diubah."),
    ("Esta versão não tem um arquivo para o seu tipo de instalação ({0}).", "Rilis ini tidak memiliki berkas untuk jenis instalasi Anda ({0})."),
    ("Caminho do AppImage inválido.", "Jalur AppImage tidak valid."),
    ("Caminho do aplicativo inválido.", "Jalur aplikasi tidak valid."),
    ("O endereço do arquivo não pertence às versões oficiais do whatRust.", "Alamat berkas bukan salah satu rilis resmi whatRust."),
    ("Não foi possível criar uma pasta temporária: {0}", "Tidak dapat membuat folder sementara: {0}"),
    ("{0} está danificado (a soma de verificação não confere); nada foi alterado.", "{0} rusak (checksum tidak cocok); tidak ada yang diubah."),
    ("Não foi possível substituir {0}: {1}", "Tidak dapat mengganti {0}: {1}"),
    ("Não foi possível instalar a nova versão: {0}{1}", "Tidak dapat memasang versi baru: {0}{1}"),
    ("Não foi possível descompactar a atualização.", "Pembaruan tidak dapat diekstrak."),
    ("A atualização não contém um aplicativo.", "Pembaruan tidak berisi aplikasi."),
    ("A atualização não contém um aplicativo válido.", "Pembaruan tidak berisi aplikasi yang valid."),
    ("A instalação foi cancelada.", "Pemasangan dibatalkan."),
    ("O dpkg não conseguiu instalar o pacote ({0}).", "dpkg tidak dapat memasang paket ({0})."),
    ("O Flatpak não conseguiu instalar a atualização ({0}). O pacote verificado está em {1}; instale com: flatpak install {2} --reinstall {1}", "Flatpak tidak dapat memasang pembaruan ({0}). Bundel terverifikasi ada di {1}; pasang dengan: flatpak install {2} --reinstall {1}"),
    ("O pkexec não foi encontrado. O pacote verificado está em {0}; instale com: sudo dpkg -i {0}", "pkexec tidak ditemukan. Paket terverifikasi ada di {0}; pasang dengan: sudo dpkg -i {0}"),
    ("Não foi possível iniciar o instalador: {0}", "Tidak dapat memulai installer: {0}"),
    ("whatRust {0} está disponível", "whatRust {0} tersedia"),
    ("Abra Configurações → Atualizações para ver as novidades e instalar.", "Buka Pengaturan → Pembaruan untuk melihat yang baru dan memasangnya."),
    ("whatRust {0} foi instalado", "whatRust {0} telah dipasang"),
    ("Reinicie o whatRust para usar a nova versão (Configurações → Atualizações).", "Mulai ulang whatRust untuk memakai versi baru (Pengaturan → Pembaruan)."),
    ("whatRust {0} está pronto para instalar", "whatRust {0} siap dipasang"),
    ("Abra Configurações → Atualizações e clique em Reiniciar e instalar.", "Buka Pengaturan → Pembaruan dan klik Mulai ulang dan pasang."),
    ("Não há atualização instalada para reiniciar.", "Tidak ada pembaruan terpasang untuk dimulai ulang."),
    ("Não foi possível reiniciar o whatRust. Abra-o manualmente.", "Tidak dapat memulai ulang whatRust. Silakan buka sendiri."),
    ("Atualizar para {0}", "Perbarui ke {0}"),
    ("whatRust — Configurações", "whatRust — Pengaturan"),
    ("whatRust — Conta destacada", "whatRust — Akun terpisah"),
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
    fn spanish_translates_and_falls_back() {
        assert_eq!(tr_in(Lang::Es, "Sair"), "Salir");
        assert_eq!(tr_in(Lang::Es, "texto sem tradução"), "texto sem tradução");
        assert_eq!(
            trf_in(Lang::Es, "Conta não encontrada: {0}", &[&"acct-1"]),
            "Cuenta no encontrada: acct-1"
        );
    }

    /// English is mandatory; every other table may lag behind (a missing key
    /// falls back to English) but must not carry stale keys, duplicates or
    /// broken placeholders.
    #[test]
    fn translation_tables_are_consistent_with_english() {
        let tables: [(&str, &[(&str, &str)]); 3] = [
            ("Spanish", ES_TABLE),
            ("Arabic", AR_TABLE),
            ("Indonesian", ID_TABLE),
        ];
        for (name, table) in tables {
            for (i, (pt, text)) in table.iter().enumerate() {
                assert!(
                    english(pt).is_some(),
                    "{name} entry with no English one: {pt}"
                );
                assert!(
                    table[i + 1..].iter().all(|(other, _)| other != pt),
                    "duplicate {name} key {pt}"
                );
                for n in 0..4 {
                    let ph = format!("{{{n}}}");
                    assert_eq!(
                        pt.contains(&ph),
                        text.contains(&ph),
                        "{name} placeholder {ph} in {pt}"
                    );
                }
            }
        }
    }

    #[test]
    fn arabic_and_indonesian_translate_and_fall_back() {
        assert_eq!(tr_in(Lang::Ar, "Sair"), "إنهاء");
        assert_eq!(tr_in(Lang::Id, "Sair"), "Keluar");
        assert_eq!(tr_in(Lang::Ar, "texto sem tradução"), "texto sem tradução");
        assert_eq!(
            trf_in(Lang::Id, "Conta não encontrada: {0}", &[&"acct-1"]),
            "Akun tidak ditemukan: acct-1"
        );
        assert_eq!(sanitize_code("ar"), AR);
        assert_eq!(sanitize_code("id-ID"), ID);
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
    fn codes_round_trip_and_unknown_means_english() {
        assert_eq!(Lang::from_code("en"), Lang::En);
        assert_eq!(Lang::from_code("EN-us"), Lang::En);
        assert_eq!(Lang::from_code("pt-BR"), Lang::PtBr);
        assert_eq!(Lang::from_code("fr"), Lang::En);
        assert_eq!(sanitize_code("zz"), EN);
        assert_eq!(sanitize_code("pt-BR"), PT_BR);
        assert_eq!(sanitize_code("en"), EN);
        assert_eq!(Lang::from_code("es"), Lang::Es);
        assert_eq!(sanitize_code("es"), ES);
    }

    #[test]
    fn os_locales_map_to_a_supported_language() {
        assert_eq!(Lang::from_code("pt_BR.UTF-8"), Lang::PtBr);
        assert_eq!(Lang::from_code("pt-PT"), Lang::PtBr);
        assert_eq!(Lang::from_code("pt"), Lang::PtBr);
        assert_eq!(Lang::from_code("en_US.UTF-8"), Lang::En);
        assert_eq!(Lang::from_code("ar-EG"), Lang::Ar);
        assert_eq!(Lang::from_code("id_ID.UTF-8"), Lang::Id);
        assert_eq!(Lang::from_code("ido"), Lang::En);
        assert_eq!(Lang::from_code("fr-FR"), Lang::En);
        assert_eq!(Lang::from_code("ptx"), Lang::En);
        assert_eq!(Lang::from_code("es_CL.UTF-8"), Lang::Es);
        assert_eq!(Lang::from_code("es-419"), Lang::Es);
        assert_eq!(Lang::from_code("es"), Lang::Es);
        assert_eq!(Lang::from_code("est"), Lang::En);
        assert_eq!(Lang::from_code(""), Lang::En);
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
            include_str!("update_service.rs"),
            include_str!("updater.rs"),
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
