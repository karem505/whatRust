"use strict";
(function(){
  const invoke=window.__TAURI__.core.invoke, isWindows=/Win/.test(navigator.platform||navigator.userAgent);
  const back=document.getElementById("back-to-chat");back.hidden=!isWindows;
  let dirty=false, diagnosticsBusy=false, timer;
  document.querySelector(".prefs").addEventListener("input",e=>{if(e.target.id!=="new_account_name")dirty=true;});
  document.getElementById("save").addEventListener("click",()=>{setTimeout(()=>{if(/^(Alterações salvas|Changes saved)/.test(document.getElementById("note").textContent))dirty=false;},400);});
  async function close(){if(dirty && !await Dlg.confirm("As alterações ainda não foram salvas.",{title:"Voltar sem salvar?",okLabel:"Voltar"}))return;await invoke("close_app_panel");}
  back.onclick=close;
  document.addEventListener("keydown",e=>{if(e.key==="Escape" && !document.querySelector('.dlg-backdrop,dialog[open],.account-actions[data-open=true]') && !e.defaultPrevented){e.preventDefault();close();}});
  const names={pending:"Aguardando envio",submitted:"Enviada ao Windows; exibição não confirmada",failed:"Falha no envio",suppressed:"Não enviada",activated:"Aviso aberto",dismissed:"Aviso encerrado"};
  const reasons={locked:"Aplicativo bloqueado",notifications_disabled:"Notificações desativadas",foreground_disabled:"Avisos em primeiro plano desativados",active_chat_disabled:"Conversa ativa",muted:"Conversa silenciada",mute_state_unknown:"Silenciamento não confirmado",windows_rejected:"Windows recusou o envio"};
  function row(title,text){const r=document.createElement("div");r.className="row";const t=document.createElement("span");t.className="row-text";const strong=document.createElement("span");strong.className="row-title";strong.textContent=title;const small=document.createElement("span");small.className="row-subtitle";small.textContent=text;t.append(strong,small);r.appendChild(t);return r;}
  async function diagnostics(){
    if(diagnosticsBusy||document.hidden)return;diagnosticsBusy=true;
    try{
      const d=await invoke("notification_diagnostics"), routes=document.getElementById("route-status"),history=document.getElementById("notification-history");
      routes.replaceChildren();history.replaceChildren();
      for(const a of d.accounts){const adapter=d.adapters["wa-"+a.id];routes.appendChild(row(a.name,t(d.native[a.id]?"Captura nativa ativa":"Captura nativa indisponível; ponte alternativa")+" · "+t(adapter?.mounted?"Controles integrados":"Acesso pelo menu da janela")+" · "+t(adapter?.detector?"Detector adicional disponível quando houver sinais confiáveis":"Aguardando conversa compatível")));}
      for(const n of d.recent.slice(0,20)){const account=d.accounts.find(a=>a.id===n.account_id)?.name||"whatRust";history.appendChild(row(new Date(n.time*1000).toLocaleTimeString(I18n.locale())+" · "+account,t(names[n.status]||n.status)+(n.reason?" — "+t(reasons[n.reason]||n.reason):"")));}
      if(!d.recent.length)history.appendChild(row("Nenhum aviso registrado","Os próximos eventos aparecerão aqui durante esta execução."));
      document.documentElement.dataset.version=d.version;
    }catch(e){document.getElementById("diagnostic-note").textContent=String(e);}finally{diagnosticsBusy=false;}
  }
  document.getElementById("test-notification").onclick=async()=>{try{document.getElementById("diagnostic-note").textContent=await invoke("test_notification");await diagnostics();}catch(e){document.getElementById("diagnostic-note").textContent=String(e);}};
  document.getElementById("windows-notifications").onclick=()=>invoke("open_notification_settings").catch(e=>{document.getElementById("diagnostic-note").textContent=String(e);});
  if(!isWindows){document.getElementById("windows-notifications").hidden=true;back.hidden=true;}
  const paths={contas:'M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2 M17 3a4 4 0 0 1 0 8 M23 21v-2a4 4 0 0 0-3-3.87 M13 7a4 4 0 1 1-8 0a4 4 0 0 1 8 0',notificacoes:'M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9 M10 21h4',aparencia:'M12 3a9 9 0 1 0 0 18a3 3 0 0 0 0-6h-1a2 2 0 0 1 0-4h5a5 5 0 0 0-4-8',seguranca:'M12 3l8 4v6c0 5-8 8-8 8s-8-3-8-8V7z M9 12l2 2l4-4',sistema:'M12 2v10 M6 5a9 9 0 1 0 12 0',atalhos:'M3 5h18v14H3z M6 9h1 M10 9h1 M14 9h1 M18 9h1 M6 13h1 M10 13h1 M14 13h4',diagnostico:'M3 12h4l3-8l4 16l3-8h4'};
  function showSection(id){if(!document.getElementById(id))id='sec-contas';for(const group of document.querySelectorAll('.prefs>.group'))group.hidden=group.id!==id;for(const link of document.querySelectorAll('.settings-nav a'))link.setAttribute('aria-current',link.hash==='#'+id?'page':'false');document.querySelector('.prefs').scrollTop=0;}
  for(const link of document.querySelectorAll('.settings-nav a')){const key=link.hash.replace('#sec-',''),svg=document.createElementNS('http://www.w3.org/2000/svg','svg'),path=document.createElementNS('http://www.w3.org/2000/svg','path');svg.setAttribute('viewBox','0 0 24 24');svg.setAttribute('aria-hidden','true');path.setAttribute('d',paths[key]);svg.appendChild(path);link.prepend(svg);link.onclick=e=>{e.preventDefault();history.replaceState(null,'',link.hash);showSection(link.hash.slice(1));};}
  showSection(location.hash.slice(1)||'sec-contas');
  diagnostics();timer=setInterval(diagnostics,3000);window.addEventListener("pagehide",()=>clearInterval(timer));
})();
