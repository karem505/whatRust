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
.\scripts\test-adapter.ps1
```

Os comandos Cargo são executados em `src-tauri`; os demais, na raiz do checkout. O teste do adaptador usa Playwright CLI com página e dados sintéticos e não acessa conversas reais.

`WHATRUST_SMOKE=1` é aceito somente por uma compilação debug. Ele usa o identificador isolado `com.karem.whatrust.smoke`, duas contas sintéticas, log separado e um caminho de relatório definido em `WHATRUST_SMOKE_REPORT`. O teste verifica o WebView2 real, movimentação da mesma visualização, origem do painel, recusa de IPC remoto, redimensionamento, bloqueio e submissão de notificação. Ele não está disponível na compilação release.

## Entrega e limites

O instalador NSIS é em português, para o usuário atual, com menu Iniciar e executável GUI sem console. A versão desta entrega é `0.6.9-local.3`, acima da versão 0.6.8 anteriormente instalada.

Os testes automatizados não substituem receber mensagens reais no aplicativo instalado, conferir conversas silenciadas em cada layout suportado, testar chamadas/microfone e confirmar arquivos arrastados/baixados com suas contas. Quando faltam sinais confiáveis, o detector adicional permanece conservador. Esses cenários devem constar como pendentes no recibo de entrega quando não executados.

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
