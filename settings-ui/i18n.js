"use strict";
// Interface language for the local pages (settings, lock screen, shell).
//
// Portuguese (Brazil) is the source language: the HTML and scripts are written
// in Portuguese and EN maps each Portuguese string to its English text. With
// English selected, text nodes and a few attributes whose (whitespace-collapsed)
// text is a key are rewritten, including nodes added later by scripts, so code
// that does `el.textContent = "Salvar"` needs no change. Strings with variable
// parts go through t("Abrir {0}", [name]). A string without an entry is shown
// as written. The start language is injected by Rust as window.__WR_LANG; the
// settings page switches it live with I18n.setLang().
(function (root) {
  var EN = {
    // titles
    "whatRust — Configurações": "whatRust — Settings",
    "whatRust — Bloqueado": "whatRust — Locked",
    // header and navigation
    "Configurações": "Settings",
    "Voltar ao WhatsApp": "Back to WhatsApp",
    "Contas": "Accounts",
    "Notificações": "Notifications",
    "Aparência": "Appearance",
    "Segurança": "Security",
    "Inicialização": "Startup",
    "Atalhos": "Shortcuts",
    "Rede": "Network",
    "Proxy": "Proxy",
    "Encaminha todo o tráfego das contas do WhatsApp por um proxy.": "Routes all WhatsApp account traffic through a proxy.",
    "Desativado": "Off",
    "Servidor": "Host",
    "Porta": "Port",
    "Proxy com usuário e senha não é suportado: os navegadores embutidos não aceitam credenciais por esse caminho. A alteração vale após reiniciar o whatRust.": "Proxies that need a username and password are not supported: the built-in browser engines do not accept credentials this way. Changes take effect after restarting whatRust.",
    "Reinicie o whatRust para aplicar o proxy.": "Restart whatRust to apply the proxy.",
    "Reiniciar agora": "Restart now",
    "Diagnóstico": "Diagnostics",
    // accounts
    "Conta ao iniciar": "Account on startup",
    "Nome da nova conta": "New account name",
    "Adicionar": "Add",
    "Múltiplas contas exigem macOS 14 ou superior.": "Multiple accounts require macOS 14 or later.",
    "Mostrar todas as contas em uma janela": "Show all accounts in one window",
    "Uma janela com um seletor de contas; as contas ocultas continuam conectadas. Requer reiniciar o whatRust.": "One window with an account switcher; hidden accounts stay connected. Requires restarting whatRust.",
    "Reinicie o whatRust para aplicar a mudança.": "Restart whatRust to apply the change.",
    "Reinicie o whatRust para aplicar. O modo somente barra de menus tem prioridade sobre esta opção.": "Restart whatRust to apply. Menu-bar-only mode takes priority over this option.",
    "Aguarde…": "Please wait…",
    "Ações de {0}": "Actions for {0}",
    "Abrir conta": "Open account",
    "Reintegrar à janela": "Return to the main window",
    "Abrir em janela separada": "Open in a separate window",
    "Usar ao iniciar": "Use on startup",
    "Renomear": "Rename",
    "Renomear conta": "Rename account",
    "Informe um nome.": "Enter a name.",
    "Mover para cima": "Move up",
    "Mover para baixo": "Move down",
    "Remover conta": "Remove account",
    "A sessão local desta conta será apagada. Para usá-la novamente, você precisará ler o QR code.": "This account's local session will be deleted. To use it again, you will need to scan the QR code.",
    "Remover \"{0}\"?": "Remove \"{0}\"?",
    "Remover": "Remove",
    "Abrir {0}": "Open {0}",
    "Em janela separada": "In a separate window",
    "{0} conversas não lidas": "{0} unread chats",
    "Opções de {0}": "Options for {0}",
    "Não foi possível carregar as contas: {0}": "Could not load the accounts: {0}",
    // updates
    "Atualizações": "Updates",
    "Sobre o whatRust": "About whatRust",
    "Contribuidores": "Contributors",
    "Obrigado a todas as pessoas que contribuíram com código para o whatRust.": "Thanks to everyone who has contributed code to whatRust.",
    "Alterar foto": "Change photo",
    "Remover foto": "Remove photo",
    "A imagem tem mais de 5 MB.": "The image is larger than 5 MB.",
    "Não foi possível ler a imagem.": "The image could not be read.",
    "Verificar atualizações automaticamente": "Check for updates automatically",
    "Consulta o GitHub (api.github.com) um minuto após iniciar e a cada 6 horas. Só pergunta qual é a versão mais recente.": "Asks GitHub (api.github.com) a minute after start and every 6 hours which version is the newest. Nothing else is sent.",
    "Instalar atualizações automaticamente": "Install updates automatically",
    "Só age quando não é preciso senha.": "Only acts where no password is needed.",
    "Versão instalada": "Installed version",
    "Verificar agora": "Check now",
    "Tipo de instalação": "Install type",
    "Última verificação": "Last check",
    "Baixar e instalar": "Download and install",
    "Reiniciar e instalar": "Restart and install",
    "Reiniciar agora": "Restart now",
    "Ver no GitHub": "View on GitHub",
    "Copiar": "Copy",
    "Novidades": "What's new",
    "Versões 0.6.10 e anteriores não atualizam sozinhas: instale esta versão manualmente uma vez. Cada arquivo baixado é conferido com a soma de verificação (SHA-256) publicada na versão antes de ser instalado.": "Versions 0.6.10 and older cannot update themselves: install this version by hand once. Every downloaded file is checked against the checksum (SHA-256) published with the release before it is installed.",
    "Nunca": "Never",
    "Baixa, confere e substitui o aplicativo em segundo plano. A nova versão começa na próxima vez que você abrir o whatRust.": "Downloads, verifies and replaces the app in the background. The new version starts the next time you open whatRust.",
    "Baixa e confere em segundo plano. O instalador roda quando você clica em Reiniciar e instalar, porque ele reinicia o whatRust.": "Downloads and verifies in the background. The installer runs when you click Restart and install, because it restarts whatRust.",
    "Não se aplica a este tipo de instalação: ela precisa de senha ou é atualizada por outra ferramenta. Você instala com um clique.": "Does not apply to this install type: it needs a password or is updated by another tool. You install with one click.",
    "Procurando atualizações…": "Looking for updates…",
    "Você está na versão mais recente": "You are on the latest version",
    "A versão {0} está disponível": "Version {0} is available",
    "Não foi possível procurar atualizações": "Could not look for updates",
    "Esta versão não tem um arquivo para o seu tipo de instalação.": "This release has no file for your install type.",
    "Baixando a versão {0}…": "Downloading version {0}…",
    "{0} MB de {1} MB": "{0} MB of {1} MB",
    "{0} MB": "{0} MB",
    "Instalando a versão {0}…": "Installing version {0}…",
    "Digite sua senha de administrador quando o sistema pedir.": "Enter your administrator password when the system asks.",
    "A versão {0} está pronta para instalar": "Version {0} is ready to install",
    "O instalador já foi baixado e conferido. O whatRust vai fechar e abrir de novo ao instalar.": "The installer has been downloaded and verified. whatRust will close and open again when it installs.",
    "A versão {0} foi instalada": "Version {0} was installed",
    "Reinicie o whatRust para usar a nova versão.": "Restart whatRust to use the new version.",
    "O instalador assumiu. O whatRust vai fechar e abrir de novo sozinho.": "The installer has taken over. whatRust will close and open again by itself.",
    "Arquivo baixado e conferido": "File downloaded and verified",
    "O pacote foi aberto na central de programas do sistema; confirme a instalação lá. Se nada abrir, execute o comando abaixo.": "The package was opened in your system's software center; confirm the installation there. If nothing opens, run the command below.",
    "O arquivo foi aberto. Termine a instalação por lá.": "The file was opened. Finish the installation there.",
    "Pede sua senha de administrador ao instalar.": "Asks for your administrator password when installing.",
    "Baixar e abrir": "Download and open",
    "Copiado ✓": "Copied ✓",
    "Selecione e copie": "Select and copy",
    // notifications
    "Receber notificações": "Receive notifications",
    "Ativa os avisos de mensagens do whatRust.": "Turns on whatRust message alerts.",
    "Som de notificação": "Notification sound",
    "Silencia os avisos sonoros de mensagens do WhatsApp. O toque de chamadas continua.": "Silences WhatsApp's message alert sounds. Calls still ring.",
    "Avisar com o aplicativo aberto": "Notify while the app is open",
    "Receber avisos mesmo com a janela em primeiro plano.": "Get alerts even when the window is in the foreground.",
    "Avisar na conversa que estou lendo": "Notify in the chat I am reading",
    "Também avisar mensagens novas na conversa aberta.": "Also alert for new messages in the open chat.",
    "Respeitar conversas silenciadas": "Respect muted chats",
    "Suspender contas sem uso": "Sleep unused accounts",
    "Libera memória de contas em segundo plano. Uma conta suspensa não recebe avisos até ser aberta de novo.": "Frees memory used by background accounts. A sleeping account gets no notifications until you open it again.",
    "Nunca": "Never",
    "Após 1 hora": "After 1 hour",
    "Após 4 horas": "After 4 hours",
    "Após 8 horas": "After 8 hours",
    "Após 24 horas": "After 24 hours",
    "Não perturbe": "Do not disturb",
    "Sem avisos do whatRust neste horário. As mensagens continuam chegando.": "No whatRust notifications during these hours. Messages still arrive.",
    "Horário do não perturbe": "Do not disturb hours",
    "Pode passar da meia-noite, por exemplo 22:00 até 08:00.": "May cross midnight, for example 22:00 to 08:00.",
    "Início do não perturbe": "Do not disturb start",
    "Fim do não perturbe": "Do not disturb end",
    "Backup das configurações": "Settings backup",
    "Salva as preferências e a lista de contas. Não inclui logins, conversas nem a senha de bloqueio.": "Saves your preferences and account list. Logins, chats and the lock password are not included.",
    "Salvar backup…": "Save backup…",
    "Restaurar backup…": "Restore backup…",
    "Backup salvo ✓": "Backup saved ✓",
    "Backup restaurado ✓": "Backup restored ✓",
    "Backup restaurado. {0} conta(s) nova(s) abrem após reiniciar e pedem o QR code.": "Backup restored. {0} new account(s) open after a restart and ask for the QR code.",
    "Reiniciar agora?": "Restart now?",
    "Reiniciar": "Restart",
    "Quando o silenciamento não puder ser confirmado, o detector adicional permanece silencioso.": "When muting cannot be confirmed, the additional detector stays silent.",
    "O detector adicional só usa mensagens novas identificáveis. Conversas silenciadas, histórico e estados desconhecidos não geram avisos adicionais.": "The additional detector only uses identifiable new messages. Muted chats, history and unknown states do not produce additional alerts.",
    // behavior
    "Comportamento": "Behavior",
    "Continuar em segundo plano ao fechar": "Keep running in the background when closed",
    "Mantém suas contas conectadas ao fechar a janela": "Keeps your accounts connected when the window is closed",
    "Iniciar minimizado": "Start minimized",
    "Iniciar sem abrir a janela principal": "Start without opening the main window",
    "Iniciar com o sistema": "Start with the system",
    "Mostrar ícone na barra de menus": "Show icon in the menu bar",
    "Quando desativado, clique no ícone do Dock para abrir o whatRust": "When off, click the Dock icon to open whatRust",
    "Iniciar minimizado com o Windows": "Start minimized with Windows",
    "Vale apenas ao entrar no Windows. Ao abrir pelo atalho, a janela sempre aparece.": "Applies only at Windows sign-in. When opened from a shortcut, the window always appears.",
    // appearance and language
    "Ícone do aplicativo": "App icon",
    "Personalizado": "Custom",
    "Escolher PNG…": "Choose PNG…",
    "Restaurar padrão": "Reset to default",
    "Imagem PNG quadrada, de 32 a 1024 px e até 5 MB. Vale para o aplicativo todo; a bandeja não muda. No Linux com Wayland, o ícone da janela vem do arquivo .desktop e não muda.": "Square PNG image, 32 to 1024 px and up to 5 MB. Applies to the whole app; the tray icon does not change. On Linux with Wayland the window icon comes from the .desktop file and does not change.",
    "Pasta de downloads": "Download folder",
    "Pasta Downloads do sistema": "System Downloads folder",
    "Escolher pasta…": "Choose folder…",
    "Restaurar Downloads": "Reset to Downloads",
    "Tamanho da interface": "Interface size",
    "Barra de título": "Title bar",
    "Gerenciadores de janelas lado a lado, como o Hyprland, não usam barra de título.": "Tiling window managers such as Hyprland don't use a title bar.",
    "Renderização": "Rendering",
    "Ocultar anúncios do app do WhatsApp": "Hide WhatsApp app promotions",
    "Controles do whatRust no cabeçalho do WhatsApp": "whatRust controls in WhatsApp's header",
    "Mostra o seletor de contas e a engrenagem dentro do cabeçalho do WhatsApp (Windows e janela única). Desative para não alterar a página. Vale ao reiniciar o whatRust.": "Shows the account switcher and gear inside WhatsApp's header (Windows and single-window mode). Turn off to leave the page untouched. Applies after restarting whatRust.",
    "Esconde os convites para baixar o WhatsApp para Windows ou Mac. Vale ao reiniciar o whatRust.": "Hides the prompts to download WhatsApp for Windows or Mac. Applies after restarting whatRust.",
    "Use o modo compatível se a janela ficar em branco em placas de vídeo antigas. Vale ao reiniciar o whatRust.": "Use Compatible if the window stays blank on an old graphics card. Applies after restarting whatRust.",
    "Compatível (sem GPU)": "Compatible (no GPU)",
    "Placa de vídeo antiga detectada: o modo automático já renderiza sem GPU.": "Old graphics card detected: Automatic already renders without the GPU.",
    "Detectado: {0}. No modo automático a barra fica oculta.": "Detected: {0}. In automatic mode the bar is hidden.",
    "Automático": "Automatic",
    "Mostrar": "Show",
    "Ocultar": "Hide",
    "Ajusta o tamanho do WhatsApp sem alterar as outras contas.": "Adjusts the size of WhatsApp without changing the other accounts.",
    "Muito pequeno": "Very small",
    "Pequeno": "Small",
    "Padrão": "Default",
    "Grande": "Large",
    "Somente na barra de menus": "Menu bar only",
    "Sem ícone no Dock nem janela principal — todas as contas continuam conectadas para notificações e cada uma abre como painel pela bandeja": "No Dock icon or main window — every account stays connected for notifications, and each opens as its own panel from the tray",
    "O ícone da bandeja é o único ponto de entrada no modo somente barra de menus, por isso permanece visível": "The tray icon is the only entry point in menu-bar-only mode, so it stays visible",
    "Requer macOS 14 (Sonoma) ou superior — este Mac não consegue manter um painel oculto conectado": "Requires macOS 14 (Sonoma) or later — this Mac cannot keep a hidden panel connected",
    "Ocultar ícone do Dock": "Hide Dock icon",
    "Mantém a janela principal; o ícone da barra de menus passa a ser a forma de reabrir o whatRust": "Keeps the main window; the menu bar icon becomes the way to reopen whatRust",
    "Idioma": "Language",
    "Idioma da interface do whatRust.": "Language of the whatRust interface.",
    // shortcuts
    "Atalho para mostrar ou ocultar": "Shortcut to show or hide",
    "Uma combinação de teclas para mostrar ou ocultar o aplicativo": "A key combination to show or hide the app",
    "Atalho": "Shortcut",
    "Gravar": "Record",
    "Atalho para desfocar": "Privacy blur shortcut",
    "Liga ou desliga o desfoque de privacidade em todas as contas. Para escolher o que é desfocado, pressione Ctrl+Shift+P (⌘+Shift+P no Mac) numa janela do WhatsApp.": "Turns the privacy blur on or off in every account. To choose what gets blurred, press Ctrl+Shift+P (⌘+Shift+P on a Mac) in a WhatsApp window.",
    "No Linux/Wayland, configure": "On Linux/Wayland, set",
    "nos atalhos do sistema.": "in the system shortcuts.",
    "Pressione as teclas… (Esc cancela)": "Press the keys… (Esc cancels)",
    "Tecla não suportada. Tente outra.": "Unsupported key. Try another.",
    "Inclua Ctrl, Alt ou Shift.": "Include Ctrl, Alt or Shift.",
    // security
    "Senha": "Password",
    "Pelo menos 6 caracteres": "At least 6 characters",
    "Pelo menos 6 caracteres.": "At least 6 characters.",
    "Confirmar": "Confirm",
    "Ativar bloqueio": "Turn on lock",
    "Usar biometria": "Use biometrics",
    "Usar {0}": "Use {0}",
    "Bloquear ao iniciar": "Lock on launch",
    "Bloquear ao ocultar": "Lock when hidden",
    "Bloquear por inatividade": "Lock when idle",
    "Minutos; 0 desativa": "Minutes; 0 turns it off",
    "Bloquear agora": "Lock now",
    "Alterar senha…": "Change password…",
    "Desativar…": "Turn off…",
    "O bloqueio protege o acesso às janelas do whatRust. Ele": "The lock protects access to the whatRust windows. It",
    "não": "does not",
    "criptografa os dados no disco.": "encrypt the data on disk.",
    "Não foi possível ativar o bloqueio": "Could not turn on the lock",
    "Não foi possível bloquear": "Could not lock",
    "Senha atual": "Current password",
    "Continuar": "Continue",
    "Informe sua senha atual.": "Enter your current password.",
    "Nova senha": "New password",
    "Alterar": "Change",
    "Informe uma nova senha.": "Enter a new password.",
    "Sua senha de bloqueio foi alterada.": "Your lock password was changed.",
    "Senha alterada": "Password changed",
    "Não foi possível alterar a senha": "Could not change the password",
    "Informe sua senha atual para desativar o bloqueio.": "Enter your current password to turn off the lock.",
    "Desativar bloqueio": "Turn off lock",
    "Desativar": "Turn off",
    "Não foi possível desativar o bloqueio": "Could not turn off the lock",
    "Não foi possível alterar a biometria": "Could not change biometrics",
    "Não foi possível salvar": "Could not save",
    // diagnostics
    "Diagnóstico de notificações": "Notification diagnostics",
    "Verificar avisos do Windows": "Check Windows alerts",
    "O teste confirma o envio; confira se o Windows exibe o banner.": "The test confirms sending; check that Windows shows the banner.",
    "Testar": "Test",
    "Permissões e Não perturbe": "Permissions and Do Not Disturb",
    "Abrir no Windows": "Open in Windows",
    "Atividade desta sessão": "Activity this session",
    "Somente horário, conta e resultado. Não armazena o texto das mensagens.": "Time, account and result only. Message text is not stored.",
    "Aguardando envio": "Waiting to send",
    "Enviada ao Windows; exibição não confirmada": "Sent to Windows; display not confirmed",
    "Falha no envio": "Send failed",
    "Não enviada": "Not sent",
    "Aviso aberto": "Alert opened",
    "Aviso encerrado": "Alert dismissed",
    "Aplicativo bloqueado": "App locked",
    "Notificações desativadas": "Notifications off",
    "Avisos em primeiro plano desativados": "Foreground alerts off",
    "Conversa ativa": "Active chat",
    "Conversa silenciada": "Muted chat",
    "Silenciamento não confirmado": "Mute state not confirmed",
    "Windows recusou o envio": "Windows rejected the send",
    "Captura nativa ativa": "Native capture active",
    "Captura nativa indisponível; ponte alternativa": "Native capture unavailable; fallback bridge",
    "Controles integrados": "Integrated controls",
    "Acesso pelo menu da janela": "Access from the window menu",
    "Detector adicional disponível quando houver sinais confiáveis": "Additional detector available when reliable signals exist",
    "Aguardando conversa compatível": "Waiting for a compatible chat",
    "Nenhum aviso registrado": "No alerts recorded",
    "Os próximos eventos aparecerão aqui durante esta execução.": "Upcoming events will appear here during this run.",
    // footer and panel
    "Salvar": "Save",
    "Alterações salvas ✓": "Changes saved ✓",
    "Alterações salvas": "Changes saved",
    "Salvo com observação: {0}": "Saved with a note: {0}",
    "Voltar sem salvar?": "Go back without saving?",
    "As alterações ainda não foram salvas.": "Your changes have not been saved.",
    "Voltar": "Back",
    // shell
    "Controles do whatRust": "whatRust controls",
    "Configurações do whatRust": "whatRust settings",
    "Trocar conta do WhatsApp": "Switch WhatsApp account",
    "Escolha uma conta para continuar": "Choose an account to continue",
    "Use o seletor acima para abrir um WhatsApp ou as configurações para gerenciar suas contas.": "Use the selector above to open a WhatsApp, or settings to manage your accounts.",
    // lock screen
    "Digite sua senha para acessar suas contas do WhatsApp.": "Enter your password to access your WhatsApp accounts.",
    "Digite sua senha": "Enter your password",
    "Digite sua senha.": "Enter your password.",
    "Desbloquear": "Unlock",
    "Esqueceu a senha? Redefinir…": "Forgot your password? Reset…",
    "Senha incorreta. Tente novamente.": "Wrong password. Try again.",
    "Não foi possível desbloquear. Tente novamente.": "Could not unlock. Try again.",
    "Não foi possível confirmar a biometria. Digite sua senha.": "Could not confirm biometrics. Enter your password.",
    "Muitas tentativas. Tente de novo em {0}s.": "Too many attempts. Try again in {0}s.",
    "Isso desconecta todas as contas deste aplicativo e remove o bloqueio. Você precisará vincular o WhatsApp novamente pelo QR Code. Suas conversas continuam no celular.": "This signs out every account in this app and removes the lock. You will need to link WhatsApp again with the QR code. Your chats stay on your phone.",
    "Redefinir o whatRust?": "Reset whatRust?",
    "Redefinir": "Reset",
    "Não foi possível redefinir o aplicativo. Tente novamente.": "Could not reset the app. Try again.",
    // dialogs
    "Cancelar": "Cancel",
    "Este campo não pode ficar vazio.": "This field cannot be empty."
  };


  // Spanish text for each Portuguese source string, same keys as EN. A key
  // missing here falls back to English (i18n.test.mjs checks none is).
  var ES = {
    "whatRust — Configurações": "whatRust — Configuración",
    "whatRust — Bloqueado": "whatRust — Bloqueado",
    "Configurações": "Configuración",
    "Voltar ao WhatsApp": "Volver a WhatsApp",
    "Contas": "Cuentas",
    "Notificações": "Notificaciones",
    "Aparência": "Apariencia",
    "Segurança": "Seguridad",
    "Inicialização": "Inicio",
    "Atalhos": "Atajos",
    "Rede": "Red",
    "Proxy": "Proxy",
    "Encaminha todo o tráfego das contas do WhatsApp por um proxy.": "Envía todo el tráfico de las cuentas de WhatsApp a través de un proxy.",
    "Desativado": "Desactivado",
    "Servidor": "Servidor",
    "Porta": "Puerto",
    "Proxy com usuário e senha não é suportado: os navegadores embutidos não aceitam credenciais por esse caminho. A alteração vale após reiniciar o whatRust.": "No se admiten proxies con usuario y contraseña: los navegadores integrados no aceptan credenciales por esta vía. El cambio se aplica al reiniciar whatRust.",
    "Reinicie o whatRust para aplicar o proxy.": "Reinicia whatRust para aplicar el proxy.",
    "Reiniciar agora": "Reiniciar ahora",
    "Diagnóstico": "Diagnóstico",
    "Conta ao iniciar": "Cuenta al iniciar",
    "Nome da nova conta": "Nombre de la nueva cuenta",
    "Adicionar": "Agregar",
    "Múltiplas contas exigem macOS 14 ou superior.": "Las cuentas múltiples requieren macOS 14 o posterior.",
    "Mostrar todas as contas em uma janela": "Mostrar todas las cuentas en una ventana",
    "Uma janela com um seletor de contas; as contas ocultas continuam conectadas. Requer reiniciar o whatRust.": "Una ventana con un selector de cuentas; las cuentas ocultas siguen conectadas. Requiere reiniciar whatRust.",
    "Reinicie o whatRust para aplicar a mudança.": "Reinicia whatRust para aplicar el cambio.",
    "Reinicie o whatRust para aplicar. O modo somente barra de menus tem prioridade sobre esta opção.": "Reinicia whatRust para aplicarlo. El modo solo barra de menús tiene prioridad sobre esta opción.",
    "Aguarde…": "Espera…",
    "Ações de {0}": "Acciones de {0}",
    "Abrir conta": "Abrir cuenta",
    "Reintegrar à janela": "Volver a la ventana principal",
    "Abrir em janela separada": "Abrir en una ventana aparte",
    "Usar ao iniciar": "Usar al iniciar",
    "Renomear": "Renombrar",
    "Renomear conta": "Renombrar cuenta",
    "Informe um nome.": "Escribe un nombre.",
    "Mover para cima": "Subir",
    "Mover para baixo": "Bajar",
    "Remover conta": "Eliminar cuenta",
    "A sessão local desta conta será apagada. Para usá-la novamente, você precisará ler o QR code.": "Se borrará la sesión local de esta cuenta. Para volver a usarla, tendrás que escanear el código QR.",
    "Remover \"{0}\"?": "¿Eliminar \"{0}\"?",
    "Remover": "Eliminar",
    "Abrir {0}": "Abrir {0}",
    "Em janela separada": "En una ventana aparte",
    "{0} conversas não lidas": "{0} chats sin leer",
    "Opções de {0}": "Opciones de {0}",
    "Não foi possível carregar as contas: {0}": "No se pudieron cargar las cuentas: {0}",
    "Atualizações": "Actualizaciones",
    "Sobre o whatRust": "Acerca de whatRust",
    "Contribuidores": "Colaboradores",
    "Obrigado a todas as pessoas que contribuíram com código para o whatRust.": "Gracias a todas las personas que han aportado código a whatRust.",
    "Alterar foto": "Cambiar foto",
    "Remover foto": "Quitar foto",
    "A imagem tem mais de 5 MB.": "La imagen pesa más de 5 MB.",
    "Não foi possível ler a imagem.": "No se pudo leer la imagen.",
    "Verificar atualizações automaticamente": "Buscar actualizaciones automáticamente",
    "Consulta o GitHub (api.github.com) um minuto após iniciar e a cada 6 horas. Só pergunta qual é a versão mais recente.": "Consulta GitHub (api.github.com) un minuto después de iniciar y cada 6 horas. Solo pregunta cuál es la versión más reciente.",
    "Instalar atualizações automaticamente": "Instalar actualizaciones automáticamente",
    "Só age quando não é preciso senha.": "Solo actúa cuando no hace falta contraseña.",
    "Versão instalada": "Versión instalada",
    "Verificar agora": "Buscar ahora",
    "Tipo de instalação": "Tipo de instalación",
    "Última verificação": "Última búsqueda",
    "Baixar e instalar": "Descargar e instalar",
    "Reiniciar e instalar": "Reiniciar e instalar",
    "Ver no GitHub": "Ver en GitHub",
    "Copiar": "Copiar",
    "Novidades": "Novedades",
    "Versões 0.6.10 e anteriores não atualizam sozinhas: instale esta versão manualmente uma vez. Cada arquivo baixado é conferido com a soma de verificação (SHA-256) publicada na versão antes de ser instalado.": "Las versiones 0.6.10 y anteriores no se actualizan solas: instala esta versión a mano una vez. Cada archivo descargado se comprueba con la suma de verificación (SHA-256) publicada en la versión antes de instalarse.",
    "Nunca": "Nunca",
    "Baixa, confere e substitui o aplicativo em segundo plano. A nova versão começa na próxima vez que você abrir o whatRust.": "Descarga, comprueba y reemplaza la aplicación en segundo plano. La nueva versión se usa la próxima vez que abras whatRust.",
    "Baixa e confere em segundo plano. O instalador roda quando você clica em Reiniciar e instalar, porque ele reinicia o whatRust.": "Descarga y comprueba en segundo plano. El instalador se ejecuta cuando haces clic en Reiniciar e instalar, porque reinicia whatRust.",
    "Não se aplica a este tipo de instalação: ela precisa de senha ou é atualizada por outra ferramenta. Você instala com um clique.": "No se aplica a este tipo de instalación: necesita contraseña o la actualiza otra herramienta. La instalas con un clic.",
    "Procurando atualizações…": "Buscando actualizaciones…",
    "Você está na versão mais recente": "Tienes la versión más reciente",
    "A versão {0} está disponível": "La versión {0} está disponible",
    "Não foi possível procurar atualizações": "No se pudieron buscar actualizaciones",
    "Esta versão não tem um arquivo para o seu tipo de instalação.": "Esta versión no tiene un archivo para tu tipo de instalación.",
    "Baixando a versão {0}…": "Descargando la versión {0}…",
    "{0} MB de {1} MB": "{0} MB de {1} MB",
    "{0} MB": "{0} MB",
    "Instalando a versão {0}…": "Instalando la versión {0}…",
    "Digite sua senha de administrador quando o sistema pedir.": "Escribe tu contraseña de administrador cuando el sistema la pida.",
    "A versão {0} está pronta para instalar": "La versión {0} está lista para instalar",
    "O instalador já foi baixado e conferido. O whatRust vai fechar e abrir de novo ao instalar.": "El instalador ya se descargó y comprobó. whatRust se cerrará y volverá a abrirse al instalar.",
    "A versão {0} foi instalada": "Se instaló la versión {0}",
    "Reinicie o whatRust para usar a nova versão.": "Reinicia whatRust para usar la nueva versión.",
    "O instalador assumiu. O whatRust vai fechar e abrir de novo sozinho.": "El instalador tomó el control. whatRust se cerrará y volverá a abrirse solo.",
    "Arquivo baixado e conferido": "Archivo descargado y comprobado",
    "O pacote foi aberto na central de programas do sistema; confirme a instalação lá. Se nada abrir, execute o comando abaixo.": "El paquete se abrió en el centro de software del sistema; confirma la instalación allí. Si no se abre nada, ejecuta el comando de abajo.",
    "O arquivo foi aberto. Termine a instalação por lá.": "El archivo se abrió. Termina la instalación allí.",
    "Pede sua senha de administrador ao instalar.": "Pide tu contraseña de administrador al instalar.",
    "Baixar e abrir": "Descargar y abrir",
    "Copiado ✓": "Copiado ✓",
    "Selecione e copie": "Selecciona y copia",
    "Receber notificações": "Recibir notificaciones",
    "Ativa os avisos de mensagens do whatRust.": "Activa los avisos de mensajes de whatRust.",
    "Som de notificação": "Sonido de notificación",
    "Silencia os avisos sonoros de mensagens do WhatsApp. O toque de chamadas continua.": "Silencia los avisos sonoros de mensajes de WhatsApp. El tono de las llamadas sigue sonando.",
    "Avisar com o aplicativo aberto": "Avisar con la aplicación abierta",
    "Receber avisos mesmo com a janela em primeiro plano.": "Recibir avisos aunque la ventana esté en primer plano.",
    "Avisar na conversa que estou lendo": "Avisar en el chat que estoy leyendo",
    "Também avisar mensagens novas na conversa aberta.": "Avisar también de mensajes nuevos en el chat abierto.",
    "Respeitar conversas silenciadas": "Respetar chats silenciados",
    "Suspender contas sem uso": "Suspender cuentas sin uso",
    "Libera memória de contas em segundo plano. Uma conta suspensa não recebe avisos até ser aberta de novo.": "Libera la memoria de las cuentas en segundo plano. Una cuenta suspendida no recibe avisos hasta que la vuelvas a abrir.",
    "Após 1 hora": "Después de 1 hora",
    "Após 4 horas": "Después de 4 horas",
    "Após 8 horas": "Después de 8 horas",
    "Após 24 horas": "Después de 24 horas",
    "Não perturbe": "No molestar",
    "Sem avisos do whatRust neste horário. As mensagens continuam chegando.": "Sin avisos de whatRust en este horario. Los mensajes siguen llegando.",
    "Horário do não perturbe": "Horario de No molestar",
    "Pode passar da meia-noite, por exemplo 22:00 até 08:00.": "Puede pasar de la medianoche, por ejemplo de 22:00 a 08:00.",
    "Início do não perturbe": "Inicio de No molestar",
    "Fim do não perturbe": "Fin de No molestar",
    "Backup das configurações": "Copia de seguridad de la configuración",
    "Salva as preferências e a lista de contas. Não inclui logins, conversas nem a senha de bloqueio.": "Guarda las preferencias y la lista de cuentas. No incluye inicios de sesión, chats ni la contraseña de bloqueo.",
    "Salvar backup…": "Guardar copia…",
    "Restaurar backup…": "Restaurar copia…",
    "Backup salvo ✓": "Copia guardada ✓",
    "Backup restaurado ✓": "Copia restaurada ✓",
    "Backup restaurado. {0} conta(s) nova(s) abrem após reiniciar e pedem o QR code.": "Copia restaurada. {0} cuenta(s) nueva(s) se abren al reiniciar y piden el código QR.",
    "Reiniciar agora?": "¿Reiniciar ahora?",
    "Reiniciar": "Reiniciar",
    "Quando o silenciamento não puder ser confirmado, o detector adicional permanece silencioso.": "Cuando no se puede confirmar el silencio, el detector adicional no avisa.",
    "O detector adicional só usa mensagens novas identificáveis. Conversas silenciadas, histórico e estados desconhecidos não geram avisos adicionais.": "El detector adicional solo usa mensajes nuevos identificables. Los chats silenciados, el historial y los estados desconocidos no generan avisos adicionales.",
    "Comportamento": "Comportamiento",
    "Continuar em segundo plano ao fechar": "Seguir en segundo plano al cerrar",
    "Mantém suas contas conectadas ao fechar a janela": "Mantiene tus cuentas conectadas al cerrar la ventana",
    "Iniciar minimizado": "Iniciar minimizado",
    "Iniciar sem abrir a janela principal": "Iniciar sin abrir la ventana principal",
    "Iniciar com o sistema": "Iniciar con el sistema",
    "Mostrar ícone na barra de menus": "Mostrar icono en la barra de menús",
    "Quando desativado, clique no ícone do Dock para abrir o whatRust": "Si está desactivado, haz clic en el icono del Dock para abrir whatRust",
    "Iniciar minimizado com o Windows": "Iniciar minimizado con Windows",
    "Vale apenas ao entrar no Windows. Ao abrir pelo atalho, a janela sempre aparece.": "Solo se aplica al iniciar sesión en Windows. Si lo abres desde un acceso directo, la ventana siempre aparece.",
    "Ícone do aplicativo": "Icono de la aplicación",
    "Personalizado": "Personalizado",
    "Escolher PNG…": "Elegir PNG…",
    "Restaurar padrão": "Restaurar predeterminado",
    "Imagem PNG quadrada, de 32 a 1024 px e até 5 MB. Vale para o aplicativo todo; a bandeja não muda. No Linux com Wayland, o ícone da janela vem do arquivo .desktop e não muda.": "Imagen PNG cuadrada, de 32 a 1024 px y hasta 5 MB. Se aplica a toda la aplicación; el icono de la bandeja no cambia. En Linux con Wayland, el icono de la ventana viene del archivo .desktop y no cambia.",
    "Pasta de downloads": "Carpeta de descargas",
    "Pasta Downloads do sistema": "Carpeta Descargas del sistema",
    "Escolher pasta…": "Elegir carpeta…",
    "Restaurar Downloads": "Restaurar Descargas",
    "Tamanho da interface": "Tamaño de la interfaz",
    "Barra de título": "Barra de título",
    "Gerenciadores de janelas lado a lado, como o Hyprland, não usam barra de título.": "Los gestores de ventanas en mosaico, como Hyprland, no usan barra de título.",
    "Renderização": "Renderizado",
    "Ocultar anúncios do app do WhatsApp": "Ocultar anuncios de la app de WhatsApp",
    "Controles do whatRust no cabeçalho do WhatsApp": "Controles de whatRust en el encabezado de WhatsApp",
    "Mostra o seletor de contas e a engrenagem dentro do cabeçalho do WhatsApp (Windows e janela única). Desative para não alterar a página. Vale ao reiniciar o whatRust.": "Muestra el selector de cuentas y el engranaje dentro del encabezado de WhatsApp (Windows y ventana única). Desactívalo para no modificar la página. Se aplica al reiniciar whatRust.",
    "Esconde os convites para baixar o WhatsApp para Windows ou Mac. Vale ao reiniciar o whatRust.": "Oculta las invitaciones a descargar WhatsApp para Windows o Mac. Se aplica al reiniciar whatRust.",
    "Use o modo compatível se a janela ficar em branco em placas de vídeo antigas. Vale ao reiniciar o whatRust.": "Usa el modo compatible si la ventana queda en blanco con tarjetas gráficas antiguas. Se aplica al reiniciar whatRust.",
    "Compatível (sem GPU)": "Compatible (sin GPU)",
    "Placa de vídeo antiga detectada: o modo automático já renderiza sem GPU.": "Tarjeta gráfica antigua detectada: el modo automático ya renderiza sin GPU.",
    "Detectado: {0}. No modo automático a barra fica oculta.": "Detectado: {0}. En modo automático la barra queda oculta.",
    "Automático": "Automático",
    "Mostrar": "Mostrar",
    "Ocultar": "Ocultar",
    "Ajusta o tamanho do WhatsApp sem alterar as outras contas.": "Ajusta el tamaño de WhatsApp sin cambiar las demás cuentas.",
    "Muito pequeno": "Muy pequeño",
    "Pequeno": "Pequeño",
    "Padrão": "Predeterminado",
    "Grande": "Grande",
    "Somente na barra de menus": "Solo en la barra de menús",
    "Sem ícone no Dock nem janela principal — todas as contas continuam conectadas para notificações e cada uma abre como painel pela bandeja": "Sin icono en el Dock ni ventana principal — todas las cuentas siguen conectadas para las notificaciones y cada una se abre como panel desde la bandeja",
    "O ícone da bandeja é o único ponto de entrada no modo somente barra de menus, por isso permanece visível": "El icono de la bandeja es el único punto de entrada en el modo solo barra de menús, por eso sigue visible",
    "Requer macOS 14 (Sonoma) ou superior — este Mac não consegue manter um painel oculto conectado": "Requiere macOS 14 (Sonoma) o posterior — este Mac no puede mantener conectado un panel oculto",
    "Ocultar ícone do Dock": "Ocultar icono del Dock",
    "Mantém a janela principal; o ícone da barra de menus passa a ser a forma de reabrir o whatRust": "Mantiene la ventana principal; el icono de la barra de menús pasa a ser la forma de volver a abrir whatRust",
    "Idioma": "Idioma",
    "Idioma da interface do whatRust.": "Idioma de la interfaz de whatRust.",
    "Atalho para mostrar ou ocultar": "Atajo para mostrar u ocultar",
    "Uma combinação de teclas para mostrar ou ocultar o aplicativo": "Una combinación de teclas para mostrar u ocultar la aplicación",
    "Atalho": "Atajo",
    "Gravar": "Grabar",
    "Atalho para desfocar": "Atajo para desenfocar",
    "Liga ou desliga o desfoque de privacidade em todas as contas. Para escolher o que é desfocado, pressione Ctrl+Shift+P (⌘+Shift+P no Mac) numa janela do WhatsApp.": "Activa o desactiva el desenfoque de privacidad en todas las cuentas. Para elegir qué se desenfoca, pulsa Ctrl+Shift+P (⌘+Shift+P en Mac) en una ventana de WhatsApp.",
    "No Linux/Wayland, configure": "En Linux/Wayland, configura",
    "nos atalhos do sistema.": "en los atajos del sistema.",
    "Pressione as teclas… (Esc cancela)": "Pulsa las teclas… (Esc cancela)",
    "Tecla não suportada. Tente outra.": "Tecla no admitida. Prueba otra.",
    "Inclua Ctrl, Alt ou Shift.": "Incluye Ctrl, Alt o Shift.",
    "Senha": "Contraseña",
    "Pelo menos 6 caracteres": "Al menos 6 caracteres",
    "Pelo menos 6 caracteres.": "Al menos 6 caracteres.",
    "Confirmar": "Confirmar",
    "Ativar bloqueio": "Activar bloqueo",
    "Usar biometria": "Usar biometría",
    "Usar {0}": "Usar {0}",
    "Bloquear ao iniciar": "Bloquear al iniciar",
    "Bloquear ao ocultar": "Bloquear al ocultar",
    "Bloquear por inatividade": "Bloquear por inactividad",
    "Minutos; 0 desativa": "Minutos; 0 lo desactiva",
    "Bloquear agora": "Bloquear ahora",
    "Alterar senha…": "Cambiar contraseña…",
    "Desativar…": "Desactivar…",
    "O bloqueio protege o acesso às janelas do whatRust. Ele": "El bloqueo protege el acceso a las ventanas de whatRust.",
    "não": "No",
    "criptografa os dados no disco.": "cifra los datos en el disco.",
    "Não foi possível ativar o bloqueio": "No se pudo activar el bloqueo",
    "Não foi possível bloquear": "No se pudo bloquear",
    "Senha atual": "Contraseña actual",
    "Continuar": "Continuar",
    "Informe sua senha atual.": "Escribe tu contraseña actual.",
    "Nova senha": "Nueva contraseña",
    "Alterar": "Cambiar",
    "Informe uma nova senha.": "Escribe una nueva contraseña.",
    "Sua senha de bloqueio foi alterada.": "Se cambió tu contraseña de bloqueo.",
    "Senha alterada": "Contraseña cambiada",
    "Não foi possível alterar a senha": "No se pudo cambiar la contraseña",
    "Informe sua senha atual para desativar o bloqueio.": "Escribe tu contraseña actual para desactivar el bloqueo.",
    "Desativar bloqueio": "Desactivar bloqueo",
    "Desativar": "Desactivar",
    "Não foi possível desativar o bloqueio": "No se pudo desactivar el bloqueo",
    "Não foi possível alterar a biometria": "No se pudo cambiar la biometría",
    "Não foi possível salvar": "No se pudo guardar",
    "Diagnóstico de notificações": "Diagnóstico de notificaciones",
    "Verificar avisos do Windows": "Comprobar avisos de Windows",
    "O teste confirma o envio; confira se o Windows exibe o banner.": "La prueba confirma el envío; revisa que Windows muestre el banner.",
    "Testar": "Probar",
    "Permissões e Não perturbe": "Permisos y No molestar",
    "Abrir no Windows": "Abrir en Windows",
    "Atividade desta sessão": "Actividad de esta sesión",
    "Somente horário, conta e resultado. Não armazena o texto das mensagens.": "Solo hora, cuenta y resultado. No guarda el texto de los mensajes.",
    "Aguardando envio": "Esperando envío",
    "Enviada ao Windows; exibição não confirmada": "Enviada a Windows; no se confirmó que se mostrara",
    "Falha no envio": "Error al enviar",
    "Não enviada": "No enviada",
    "Aviso aberto": "Aviso abierto",
    "Aviso encerrado": "Aviso descartado",
    "Aplicativo bloqueado": "Aplicación bloqueada",
    "Notificações desativadas": "Notificaciones desactivadas",
    "Avisos em primeiro plano desativados": "Avisos en primer plano desactivados",
    "Conversa ativa": "Chat activo",
    "Conversa silenciada": "Chat silenciado",
    "Silenciamento não confirmado": "Silencio no confirmado",
    "Windows recusou o envio": "Windows rechazó el envío",
    "Captura nativa ativa": "Captura nativa activa",
    "Captura nativa indisponível; ponte alternativa": "Captura nativa no disponible; puente alternativo",
    "Controles integrados": "Controles integrados",
    "Acesso pelo menu da janela": "Acceso desde el menú de la ventana",
    "Detector adicional disponível quando houver sinais confiáveis": "Detector adicional disponible cuando haya señales confiables",
    "Aguardando conversa compatível": "Esperando un chat compatible",
    "Nenhum aviso registrado": "No hay avisos registrados",
    "Os próximos eventos aparecerão aqui durante esta execução.": "Los próximos eventos aparecerán aquí durante esta ejecución.",
    "Salvar": "Guardar",
    "Alterações salvas ✓": "Cambios guardados ✓",
    "Alterações salvas": "Cambios guardados",
    "Salvo com observação: {0}": "Guardado con una observación: {0}",
    "Voltar sem salvar?": "¿Volver sin guardar?",
    "As alterações ainda não foram salvas.": "Los cambios todavía no se han guardado.",
    "Voltar": "Volver",
    "Controles do whatRust": "Controles de whatRust",
    "Configurações do whatRust": "Configuración de whatRust",
    "Trocar conta do WhatsApp": "Cambiar de cuenta de WhatsApp",
    "Escolha uma conta para continuar": "Elige una cuenta para continuar",
    "Use o seletor acima para abrir um WhatsApp ou as configurações para gerenciar suas contas.": "Usa el selector de arriba para abrir un WhatsApp, o la configuración para administrar tus cuentas.",
    "Digite sua senha para acessar suas contas do WhatsApp.": "Escribe tu contraseña para acceder a tus cuentas de WhatsApp.",
    "Digite sua senha": "Escribe tu contraseña",
    "Digite sua senha.": "Escribe tu contraseña.",
    "Desbloquear": "Desbloquear",
    "Esqueceu a senha? Redefinir…": "¿Olvidaste la contraseña? Restablecer…",
    "Senha incorreta. Tente novamente.": "Contraseña incorrecta. Inténtalo de nuevo.",
    "Não foi possível desbloquear. Tente novamente.": "No se pudo desbloquear. Inténtalo de nuevo.",
    "Não foi possível confirmar a biometria. Digite sua senha.": "No se pudo confirmar la biometría. Escribe tu contraseña.",
    "Muitas tentativas. Tente de novo em {0}s.": "Demasiados intentos. Vuelve a intentarlo en {0} s.",
    "Isso desconecta todas as contas deste aplicativo e remove o bloqueio. Você precisará vincular o WhatsApp novamente pelo QR Code. Suas conversas continuam no celular.": "Esto cierra la sesión de todas las cuentas de esta aplicación y quita el bloqueo. Tendrás que vincular WhatsApp otra vez con el código QR. Tus chats siguen en el teléfono.",
    "Redefinir o whatRust?": "¿Restablecer whatRust?",
    "Redefinir": "Restablecer",
    "Não foi possível redefinir o aplicativo. Tente novamente.": "No se pudo restablecer la aplicación. Inténtalo de nuevo.",
    "Cancelar": "Cancelar",
    "Este campo não pode ficar vazio.": "Este campo no puede quedar vacío."
  };

  var TABLES = { en: EN, es: ES };
  var LOCALES = { "pt-BR": "pt-BR", en: "en-US", es: "es" };
  function code(c) { return c === "en" || c === "es" ? c : "pt-BR"; }
  var lang = code(root.__WR_LANG);

  function norm(s) { return String(s).replace(/\s+/g, " ").trim(); }
  function own(o, k) { return Object.prototype.hasOwnProperty.call(o, k); }
  // Text for a normalised key in the current language, or undefined to keep
  // the Portuguese source. Spanish falls back to English, then Portuguese.
  function lookup(k) {
    var table = TABLES[lang];
    if (!table) return undefined;
    if (own(table, k)) return table[k];
    return own(EN, k) ? EN[k] : undefined;
  }
  function fill(text, args) {
    if (!args) return text;
    for (var i = 0; i < args.length; i++) text = text.split("{" + i + "}").join(String(args[i]));
    return text;
  }
  // Translate a source string (exact key) and fill {0}, {1}...
  function t(s, args) {
    var key = norm(s);
    var tr = lookup(key);
    var out = tr === undefined ? s : tr;
    return fill(out, args);
  }

  var api = {
    EN: EN,
    ES: ES,
    norm: norm,
    t: t,
    lang: function () { return lang; },
    locale: function () { return LOCALES[lang]; },
    setLang: function () {},
  };
  root.I18n = api;
  root.t = t;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
  if (typeof document === "undefined" || !document.documentElement) return;

  // ---- DOM translation -------------------------------------------------
  var ATTRS = ["aria-label", "title", "placeholder"];
  var text = new WeakMap();   // text node -> { src, out }
  var attrs = new WeakMap();  // element  -> { name: { src, out } }
  var SKIP = { SCRIPT: 1, STYLE: 1, CODE: 1 };

  function translateText(node) {
    var rec = text.get(node);
    if (!rec || node.nodeValue !== rec.out) rec = { src: node.nodeValue, out: node.nodeValue };
    var key = norm(rec.src), out = rec.src;
    var tr = lookup(key);
    if (tr !== undefined) {
      var m = /^(\s*)[\s\S]*?(\s*)$/.exec(rec.src);
      out = m[1] + tr + m[2];
    }
    rec.out = out;
    text.set(node, rec);
    if (node.nodeValue !== out) node.nodeValue = out;
  }

  function translateAttrs(el) {
    var recs = attrs.get(el) || {};
    ATTRS.forEach(function (name) {
      if (!el.hasAttribute || !el.hasAttribute(name)) return;
      var value = el.getAttribute(name), rec = recs[name];
      if (!rec || value !== rec.out) rec = { src: value, out: value };
      var key = norm(rec.src), tr = lookup(key), out = tr === undefined ? rec.src : tr;
      rec.out = out;
      recs[name] = rec;
      if (value !== out) el.setAttribute(name, out);
    });
    attrs.set(el, recs);
  }

  function walk(node) {
    if (node.nodeType === 3) { translateText(node); return; }
    if (node.nodeType !== 1 || SKIP[node.nodeName]) return;
    translateAttrs(node);
    for (var c = node.firstChild; c; c = c.nextSibling) walk(c);
  }

  var observer;
  function translateAll() {
    if (observer) observer.disconnect();
    walk(document.documentElement);
    observe();
  }

  function observe() {
    if (!observer) {
      observer = new MutationObserver(function (records) {
        observer.disconnect();
        records.forEach(function (r) {
          if (r.type === "characterData") { translateText(r.target); }
          else if (r.type === "attributes") { translateAttrs(r.target); }
          else { r.addedNodes.forEach(function (n) { walk(n); }); }
        });
        observe();
      });
    }
    observer.observe(document.documentElement, {
      subtree: true, childList: true, characterData: true,
      attributes: true, attributeFilter: ATTRS,
    });
  }

  api.setLang = function (next) {
    lang = code(next);
    root.__WR_LANG = lang;
    document.documentElement.lang = lang;
    translateAll();
    root.dispatchEvent(new Event("i18n"));
  };

  function start() { document.documentElement.lang = lang; translateAll(); }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", start);
  else start();
})(typeof window !== "undefined" ? window : globalThis);
