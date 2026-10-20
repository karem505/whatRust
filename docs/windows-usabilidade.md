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

O instalador NSIS é em português, para o usuário atual, com menu Iniciar e executável GUI sem console. A versão desta entrega é `0.6.9-local.1`, acima da versão 0.6.8 anteriormente instalada.

Os testes automatizados não substituem receber mensagens reais no aplicativo instalado, conferir conversas silenciadas em cada layout suportado, testar chamadas/microfone e confirmar arquivos arrastados/baixados com suas contas. Quando faltam sinais confiáveis, o detector adicional permanece conservador. Esses cenários devem constar como pendentes no recibo de entrega quando não executados.

## Recuperação

O PR #26 e sua branch foram preservados. Antes das mudanças foi criado um bundle Git completo fora do checkout. A versão anterior do instalador permanece na pasta de entrega. Para voltar à interface anterior, reinstale a versão anterior sem selecionar exclusão de dados do aplicativo. A implementação não migra os diretórios de sessão para outro formato.
