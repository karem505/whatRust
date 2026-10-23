# Windows: interface integrada e notificações

Esta implementação usa Tauri 2.11.2 fixado e suas APIs experimentais de múltiplas visualizações. macOS e Linux conservam o fluxo de uma janela por conta.

## Estrutura

- `sessions.rs`: uma WebView viva por conta, janela principal, destaque/reintegração, painel local e acesso de recuperação pelo menu da janela.
- `integration.rs` e `resources/integration.js`: contrato mínimo por origem/visualização, dados de exibição, montagem sem classes geradas, Shadow DOM e detector conservador. A criação e o gerenciamento de visualizações só ocorrem no Rust.
- `notify.rs`: políticas, correlação por identidade, recibos de envio, falhas síncronas/assíncronas e histórico limitado a 100 eventos sem conteúdo de mensagens.

O painel de configurações é uma visualização local com armazenamento e permissões próprios. WhatsApp recebe apenas comandos de relato, exibição e abertura do painel. `Shadow DOM` isola CSS; não é fronteira de segurança.

## Compatibilidade

A conta histórica `default` continua com o armazenamento padrão. As demais preservam o mesmo diretório por ID. Destacar usa reparent da visualização existente, preservando sua sessão e estado em memória. Configurações antigas recebem as três preferências novas ativadas, mantendo `notifications` como controle global.

A preferência de conta inicial continua independente do ID histórico `default`. A ordem só aceita uma permutação completa das contas. Remover uma conta exige confirmação no painel e não permite remover a última.

## Notificações

No Windows, WebView2 é a rota principal. A ponte JavaScript assume quando essa rota está indisponível ou recusa a criação. O detector visual só complementa eventos com identificadores de mensagens recebidas reconhecidos no DOM montado da conversa atual. Não lê texto de mensagens nem estruturas internas do React. Identificadores transmitidos pelo detector são hashes em memória.

Histórico, troca de conversa, remetente próprio e reconstruções sem referência de continuidade não geram avisos adicionais. O silenciamento exige prova explícita. Sem essa prova, o reforço permanece silencioso. Em layouts compatíveis, abrir o menu da conversa permite reconhecer a ação explícita de silenciar/reativar; esse reconhecimento fica isolado no adaptador.

Uma confirmação de envio ao Windows não prova que houve banner. Não perturbe e as permissões do sistema continuam sendo respeitados. O diagnóstico distingue envio pendente, aceitação, falha, supressão, clique e encerramento. Conteúdo não é escrito no histórico nem nos logs.

## Verificações

```powershell
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
node src-tauri/resources/bridge.test.mjs
node --test src-tauri/resources/notifications.test.mjs
node settings-ui/comboToAccelerator.test.mjs
node settings-ui/dialog.test.mjs
node settings-ui/i18n.test.mjs
.\scripts\test-adapter.ps1
.\scripts\test-ui.ps1
.\scripts\test-shell.ps1
```

Os comandos Cargo são executados em `src-tauri`; os demais, na raiz do checkout. O teste do adaptador usa Playwright CLI com página e dados sintéticos e não acessa conversas reais.

`WHATRUST_SMOKE=1` é aceito somente por uma compilação debug. Ele usa o identificador isolado `com.karem.whatrust.smoke`, duas contas sintéticas, log separado e um caminho de relatório definido em `WHATRUST_SMOKE_REPORT`. O teste verifica o WebView2 real, movimentação da mesma visualização, origem do painel, recusa de IPC remoto, redimensionamento, bloqueio e submissão de notificação. Ele não está disponível na compilação release.

## Entrega e limites

O instalador NSIS é em português, para o usuário atual, com menu Iniciar e executável GUI sem console. A versão desta entrega é `0.6.9`, acima da versão 0.6.8 anteriormente instalada.

Os testes automatizados não substituem receber mensagens reais no aplicativo instalado, conferir conversas silenciadas em cada layout suportado, testar chamadas/microfone e confirmar arquivos arrastados/baixados com suas contas. Quando faltam sinais confiáveis, o detector adicional permanece conservador. Esses cenários devem constar como pendentes no recibo de entrega quando não executados.

## Idioma

O português do Brasil é o padrão. Em Configurações > Aparência > Idioma é possível trocar para English; a troca vale na hora e fica em `settings.json` (`language`). O texto-fonte do código é em português e `src-tauri/src/i18n.rs` (Rust) e `settings-ui/i18n.js` (páginas locais) trazem a tabela em inglês. Textos sem tradução aparecem em português. Os testes `cargo test` e `node settings-ui/i18n.test.mjs` falham quando um texto novo fica sem entrada em inglês. As páginas do instalador NSIS continuam em português.

## Recuperação

O PR #26 e sua branch foram preservados. Antes das mudanças foi criado um bundle Git completo fora do checkout. A versão anterior do instalador permanece na pasta de entrega. Para voltar à interface anterior, reinstale a versão anterior sem selecionar exclusão de dados do aplicativo. A implementação não migra os diretórios de sessão para outro formato.

## Correção da inicialização instalada (local.2)

O layout não altera menus nativos durante eventos de redimensionamento: `SetMenu` dispara `WM_SIZE` e o bloqueio interno de menu do Tauri pode ser acessado novamente. A visibilidade do menu muda somente quando muda a integração/conta ativa, antes de calcular o layout.

As contas Windows agora nascem como WebViews filhas, sem uma janela temporária. Isso evita a lógica de redimensionamento automático de uma WebviewWindow movida. Os limites usam pixels físicos da área cliente atual. O shell de recuperação ocupa somente sua própria faixa, fica oculto quando os controles integrados estão disponíveis e usa a janela completa apenas quando não existe visualização ativa.

Na primeira execução, o perfil `default` abre a página oficial de vinculação do WhatsApp; um perfil cadastrado não significa uma sessão autenticada. Uma configuração explicitamente vazia recebe um perfil inicial, sem apagar nem modificar outros diretórios. QR Code e login por telefone permanecem no WhatsApp. Não se lê estado privado de autenticação.

A regressão nativa alterna integração/recuperação via IPC real, confere o tamanho físico até o rodapé, redimensiona repetidamente e abre o painel antes de vincular contas. Os cenários usam perfis isolados sem login e não desconectam contas do usuário.

## Controles e estilo (local.3)

A inspeção do WhatsApp real encontrou uma barra de navegação vertical implementada como `header` com 64 px de largura antes do cabeçalho da lista. O adaptador agora verifica geometria, relação com `#side`, orientação horizontal e espaço livre. Cabeçalhos aninhados usam a linha interna de ações. Os botões próprios têm 40 px e SVGs de 24 px; não encolhem nem alteram CSS do WhatsApp. Se não couberem, usa-se o acesso nativo de recuperação. `ResizeObserver` acompanha somente a lateral relevante e a observação de tema fica limitada à raiz e ao corpo.

O menu de contas abre próximo ao botão, respeita os limites da janela e fecha por Esc ou clique externo. O tema claro/escuro resolvido pelo adaptador é transmitido apenas como um valor validado (`light`/`dark`) e aplicado à visualização local quando ela abre. Isso alinha painel, menus, diálogos e controles mesmo quando WhatsApp e Windows usam temas diferentes. A identidade do whatRust permanece indicada no painel.

As regressões incluem navegação vertical antes do cabeçalho correto, cabeçalhos aninhados, ações originais ocupando a largura, telas estreitas, recuperação e mudança de tema. O recebimento real de mensagens continua dependendo de validação pelo usuário.

## Barra de recuperação (local.4)

O shell tem uma faixa de 48 px com seletor de contas e configurações. Raiz e corpo não geram rolagem; o texto da tela completa fica oculto na faixa compacta. Seus controles e cores seguem os mesmos tamanhos e temas do WhatsApp. A rolagem das conversas e do painel de configurações não é modificada.

## Largura do seletor (local.5)

O seletor compacto acompanha a largura do nome selecionado até 320 px (ou 65% da janela). O conteúdo excedente usa reticências e o nome completo fica na dica e na opção acessível. Um elemento de medição com a mesma fonte evita que opções maiores afastem a seta de nomes curtos. A atualização local recebe o ID da conta ativa, mantendo nome e janela sincronizados.

## Menus, troca de contas e bloqueio (local.5)

Menus locais removem o `inset` padrão do popover, medem sua altura real e escolhem acima/abaixo do botão conforme o espaço. Acompanham rolagem e redimensionamento e fecham quando o botão sai do painel visível. Restauração de foco não rola o painel nem toma foco de outro menu/diálogo. Diálogos altos e avisos de download têm limites próprios; nomes extensos não ampliam o painel.

A ativação de conta é uma operação única na fila da interface: seleciona a identidade, oculta as demais visualizações, mostra a conta, aplica os limites físicos e publica o mesmo ID ao seletor. O tamanho final é aplicado depois de mostrar a visualização. Atualizações pendentes do seletor usam o ID mais recente.

A tela de senha usa SVG de cadeado, paleta neutra, botão verde e textos em português. A revisão também inclui diálogos de senha, menus nativos e avisos de download/anexo. Bloqueio e ativação compartilham a fila da interface. O desbloqueio sempre retorna do callback IPC antes de destruir o próprio WebView2; destruí-lo dentro do callback podia travar a reintegração. O painel verifica novamente o estado de bloqueio antes de revelar sua janela.

A redefinição exige confirmação e grava um marcador fora da pasta dos perfis. A limpeza só ocorre na próxima inicialização, antes de criar WebViews. Falhas mantêm o marcador e abortam a abertura dos perfis; a redefinição não declara sucesso ignorando arquivos em uso. Os testes destrutivos usam somente diretórios temporários sintéticos.

A regressão nativa usa senha sintética para validar rejeição de senha incorreta, troca de senha, bloqueio de janelas principal/destacada, recusa de desbloqueio pela página remota, biometria desativada, restauração da conta pendente, bloqueio ao fechar e desativação com senha atual. O teste visual cobre temas claro/escuro, teclado e textos em português. Autenticação real pelo Windows Hello requer interação com o dispositivo e permanece pendente; não é simulada como sucesso.

## Notificação com conversa aberta e janela minimizada (local.6)

O plugin genérico de notificações do Tauri substituía `window.Notification` por uma função que chamava `plugin:notification|notify`. A ponte capturava essa função como se fosse o construtor nativo e delegava para ela; a recusa assíncrona da ACL não acionava a alternativa. No Windows, o aplicativo não registra mais esse plugin, pois o serviço de `notify.rs` já controla permissões, bloqueio, conta e cliques. As permissões da página remota permanecem restritas. macOS/Linux conservam o plugin existente.

A ponte pede também a permissão real do navegador. Sem ela, usa diretamente o comando próprio; erros assíncronos do construtor nativo acionam a alternativa. O estado de primeiro plano exclui explicitamente janelas minimizadas. O teste nativo agora cria uma notificação pela API JavaScript real dentro do WhatsApp/WebView2, minimizado e oculto na bandeja, e exige recibo da rota WebView2. Uma chamada direta de teste ao Rust não é evidência suficiente para essa integração.

A API persistente `ServiceWorkerRegistration.showNotification()` chamada pela página usa diretamente o comando próprio: o contrato de `ICoreWebView2_24.NotificationReceived` cobre notificações não persistentes, portanto uma Promise persistente resolvida não prova que esse evento ocorreu. Referência: https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2_24 . A permissão real continua sendo solicitada para o navegador, inclusive para os contextos de worker.

As WebViews filhas também precisam de visibilidade própria: minimizar a janela nativa deixava `document.hidden === false` mesmo sem foco. O layout agora oculta controladores quando a janela está minimizada, na bandeja ou bloqueada e restaura apenas a conta/painel selecionado ao reaparecer. A minimização preserva o último tamanho útil. Há regressões para `document.hidden` e retorno à visibilidade, além do envio da notificação.

## Bloqueio silencioso, abertura pelo instalador e travamentos (local.7)

“Bloquear ao ocultar” agora protege as contas e permanece na bandeja, sem criar nem focar a tela de senha. Abrir pelo ícone, atalho, menu ou outra execução apresenta o desbloqueio. Bloqueio manual mantém a apresentação imediata. Solicitações antigas de apresentação são invalidadas quando a intenção mais recente é ocultar.

Ao bloquear, o serviço avança uma geração de eventos, cancela os avisos pendentes e remove os avisos anteriores do Windows somente deste aplicativo. A operação do sistema ocorre em workers; o loop gráfico não espera pela limpeza. O envio revalida bloqueio e geração imediatamente antes/depois de chamar o Windows. Temporizadores e conversões de ícone preservam a geração de detecção para não reaparecer após um desbloqueio rápido. A preferência global de notificações e o respeito ao silenciamento continuam valendo.

O evento CloseRequested de uma notificação do WebView2 não executa mais remoção de histórico WinRT de forma síncrona. ReportShown/Clicked/Closed são adiados para fora do callback original; nenhum método COM é chamado com a coleção de notificações emprestada. A visibilidade dos filhos só é alterada quando muda, reduzindo eventos e reentrância. Consultas de Windows Hello e os IPCs de notificações/layout são executados fora do callback gráfico. Essas alterações tratam caminhos capazes de bloquear a UI; o AppHang registrado pelo Windows confirma a ocorrência, mas não fornece a pilha necessária para atribuir todos os congelamentos a uma única causa.

O arquivo de diagnóstico anterior é preservado em `whatrust.previous.log`. Um pulso verifica a resposta da thread gráfica; atraso superior a 5 s registra o estágio técnico atual e o tempo de recuperação, sem conteúdo de conversas. Não encerra nem reinicia o processo automaticamente. O teste nativo provoca um atraso finito em um perfil isolado para validar esse diagnóstico.

A página final NSIS e `/R` passam `--show`. Esse argumento sobrepõe somente a preferência de iniciar oculto; não dispensa a senha quando há bloqueio ao iniciar. Autostart com `--minimized` continua oculto. O template está fixado no Tauri CLI 2.12.3, com origem, hash e licenças em `src-tauri/windows`; apenas as duas chamadas de execução são modificadas.

A regressão nativa inclui 12 ciclos de criar/encerrar avisos enquanto troca conta, abre/fecha painel e altera não lidas; chegada bloqueada; remoção de histórico Windows; reabertura exigindo senha; não repetição de eventos atrasados; e início explícito visível com preferência persistida de segundo plano. Todos os testes usam contas e arquivos sintéticos.

Os testes unitários desativam o arquivo de log instalado. As regressões nativas continuam usando seu próprio relatório/log isolado, evitando misturar arquivos sintéticos com evidência do aplicativo real.

## Layout deferido (local.8)

Um novo atraso real foi registrado em `account layout`. O runtime do Tauri despacha eventos com o mutex de listeners da janela adquirido; aplicar visibilidade/tamanho no callback de foco/redimensionamento permitia reentrada no mesmo caminho nativo. Esses callbacks agora apenas enfileiram um layout por janela; o fechamento também agenda suas mutações após liberar o callback. Limites físicos idênticos não são reaplicados. Mostrar uma visualização ou mover entre janelas invalida a geometria para reaplicar seu tamanho final uma vez. Essa correção é adicional às alterações de notificações da local.7.

A regressão nativa também reproduziu o travamento ao reintegrar uma visualização com layout pendente. O dispatcher Wry segura seu mutex de ID de janela enquanto aguarda a mensagem de reparent; um layout anterior na fila tentava esse mesmo mutex, bloqueando a UI antes de processar a mudança. O reparent agora ocorre na própria tarefa de UI, com a visualização previamente oculta, sem adquirir o mutex no worker que aguardará a fila. Painel e contas compartilham esse caminho.
