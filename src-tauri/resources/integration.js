(function () {
  "use strict";
  function L(pt, en) { return window.__WR_LANG === "en" ? en : pt; }
  if (!window.__WHATRUST_WINDOWS__ || location.origin !== "https://web.whatsapp.com") return;
  var state = { accounts: [], current_id: "", native_notifications: false };
  var host, root, menu, gear, accountButton, accountName, accountBadge, status, lastReport = "", observer, scheduled, resizeObserver, resizeTarget, themeObserver;
  function call(cmd, args) { var t=window.__TAURI__;if(!t||!t.core)return Promise.resolve(null);return t.core.invoke(cmd, args).catch(function () { return null; }); }
  function button(text, fn) { var b=document.createElement("button");b.type="button";b.textContent=text;b.addEventListener("click",function(e){e.stopPropagation();fn();});return b; }
  function openPanel() { if(menu.open)menu.close();call("open_app_panel"); }
  function positionMenu(){var r=accountButton.getBoundingClientRect(),below=Math.max(0,innerHeight-12-r.bottom-8),above=Math.max(0,r.top-20),natural=menu.scrollHeight+2,down=below>=natural||below>=above;menu.style.maxHeight=Math.max(40,Math.min(innerHeight-24,down?below:above))+"px";var height=menu.offsetHeight;menu.style.top=Math.max(12,Math.min(down?r.bottom+8:r.top-height-8,innerHeight-height-12))+"px";menu.style.left=Math.max(12,Math.min(r.left,innerWidth-menu.offsetWidth-12))+"px";}

  function draw() {
    if(!accountButton)return;
    var focusKey=root.activeElement && root.activeElement.dataset.item;
    var keepFocus=menu.open && !!focusKey;
    var own=state.accounts.find(function(a){return a.id===state.current_id;});
    accountName.textContent=own ? own.name : L("Contas","Accounts");
    accountBadge.textContent=own && own.unread ? String(own.unread>99?"99+":own.unread) : "";
    accountBadge.hidden=!accountBadge.textContent;
    accountButton.setAttribute("aria-label",L("Trocar conta","Switch account")+(own?": "+own.name:"")+(own&&own.unread?" · "+own.unread+L(" conversas não lidas"," unread chats"):""));
    accountButton.title=own ? L("Conta: ","Account: ")+own.name+L(" — Trocar conta"," — Switch account") : L("Trocar conta do WhatsApp","Switch WhatsApp account");
    gear.setAttribute("aria-label",L("Configurações do whatRust","whatRust settings"));gear.title=L("Configurações do whatRust","whatRust settings");menu.setAttribute("aria-label",L("Contas do whatRust","whatRust accounts"));
    menu.replaceChildren();
    var title=document.createElement("h2");title.textContent=L("Suas contas","Your accounts");menu.appendChild(title);
    state.accounts.forEach(function(a){var b=button(a.name+(a.unread?" · "+a.unread+L(" conversas não lidas"," unread chats"):""),function(){menu.close();call("switch_account",{id:a.id});});b.setAttribute("aria-current",String(a.id===state.current_id));b.dataset.item=a.id;menu.appendChild(b);});
    var manage=button(L("Gerenciar contas e configurações","Manage accounts and settings"),openPanel);manage.dataset.item="manage";menu.appendChild(manage);
    var close=button(L("Fechar","Close"),function(){menu.close();});close.dataset.item="close";menu.appendChild(close);
    status=document.createElement("p");status.setAttribute("role","status");status.textContent=own?L("Conta atual: ","Current account: ")+own.name:L("Escolha uma conta","Choose an account");menu.appendChild(status);
    if(menu.open)positionMenu();
    if(keepFocus){var restore=Array.from(menu.querySelectorAll("button")).find(function(b){return b.dataset.item===focusKey;})||menu.querySelector("button");if(restore)restore.focus();}
  }
  window.__wrRelang=function(){if(accountButton)draw();};
  window.__whatrustState=function(next){if(!next)return;state=next;window.__whatrustPreferNative=!!next.native_notifications;window.__whatrustNotificationEpoch=next.notification_epoch||0;window.__whatrustLocked=!!next.notifications_locked;draw();};
  function create(){
    host=document.createElement("span");host.id="__whatrust_controls";host.setAttribute("data-whatrust","controls");
    host.style.cssText="display:inline-flex;align-items:center;flex:0 0 auto;min-width:84px;max-width:200px;margin-inline-start:8px;align-self:center";
    root=host.attachShadow({mode:"open"});
    var style=document.createElement("style");style.textContent=`
      :host{color:inherit;font:13px system-ui}*{box-sizing:border-box}
      button{font:inherit;cursor:pointer;border:0;background:transparent;color:inherit}
      button:hover{background:light-dark(#0a0a0a0a,#ffffff14)}button:focus-visible{outline:2px solid #21c063;outline-offset:2px}
      .bar{display:flex;align-items:center;gap:4px;flex:none;height:40px}
      .bar>button{display:inline-flex;align-items:center;justify-content:center;flex:none;height:40px;min-width:40px;padding:8px;border-radius:50%}
      .bar>.account{position:relative;gap:6px;border-radius:20px}
      .account-name{max-width:80px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:500}
      :host([data-compact="true"]) .account-name{display:none}
      .badge{flex:none;min-width:16px;height:16px;padding:0 4px;border-radius:9px;background:#21c063;color:#0a0a0a;font:10px/16px system-ui;text-align:center}
      :host([data-compact="true"]) .badge{position:absolute;inset-inline-end:0;top:0}
      [hidden]{display:none!important}svg{width:24px;height:24px;flex:none;stroke:currentColor;fill:none;stroke-width:1.7;stroke-linecap:round;stroke-linejoin:round}
      dialog{color-scheme:light dark;color:light-dark(#0a0a0a,#fafafa);background:light-dark(#fff,#232626);border:1px solid #8882;border-radius:8px;box-shadow:0 2px 12px #0005;width:320px;max-width:calc(100vw - 24px);max-height:calc(100vh - 24px);overflow:auto;padding:12px 8px;inset:auto;margin:0}
      dialog::backdrop{background:transparent}dialog button{overflow-wrap:anywhere;display:block;width:100%;text-align:left;margin:5px 0;padding:10px 12px;border-radius:8px}
      h2{font-size:16px;font-weight:600;margin:4px 12px 10px}p{overflow-wrap:anywhere;font-size:12px;opacity:.65;margin:12px 12px 4px}dialog [aria-current=true]{background:#8696a01f;font-weight:600}
    `;
    var bar=document.createElement("span");bar.className="bar";
    accountButton=button("Contas",function(){draw();if(!menu.open)menu.showModal();positionMenu();accountButton.setAttribute("aria-expanded","true");var b=menu.querySelector("button");if(b)b.focus();});accountButton.className="account";accountButton.textContent="";
    var people=document.createElementNS("http://www.w3.org/2000/svg","svg"),peoplePath=document.createElementNS("http://www.w3.org/2000/svg","path");people.setAttribute("viewBox","0 0 24 24");people.setAttribute("aria-hidden","true");peoplePath.setAttribute("d","M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M16 3a4 4 0 0 1 0 8 M22 21v-2a4 4 0 0 0-3-3.87 M13 7a4 4 0 1 1-8 0a4 4 0 0 1 8 0");people.appendChild(peoplePath);
    accountName=document.createElement("span");accountName.className="account-name";
    accountBadge=document.createElement("span");accountBadge.className="badge";accountBadge.setAttribute("aria-hidden","true");
    accountButton.append(people,accountName,accountBadge);accountButton.setAttribute("aria-haspopup","dialog");accountButton.setAttribute("aria-expanded","false");
    gear=button("⚙",openPanel);gear.setAttribute("aria-label","Configurações do whatRust");gear.title="Configurações do whatRust";
    gear.textContent="";
    var svg=document.createElementNS("http://www.w3.org/2000/svg","svg"),path=document.createElementNS("http://www.w3.org/2000/svg","path");svg.setAttribute("viewBox","0 0 24 24");svg.setAttribute("aria-hidden","true");path.setAttribute("d","M9.5 3h5l.6 2.4 2 1.2 2.4-.7 2.5 4.3-1.8 1.8v2.3l1.8 1.8-2.5 4.3-2.4-.7-2 1.2-.6 2.1h-5l-.6-2.1-2-1.2-2.4.7L2 16.1l1.8-1.8V12L2 10.2l2.5-4.3 2.4.7 2-1.2z M15.5 12.5a3.5 3.5 0 1 1-7 0a3.5 3.5 0 0 1 7 0");svg.appendChild(path);gear.appendChild(svg);
    menu=document.createElement("dialog");menu.setAttribute("aria-label","Contas do whatRust");menu.addEventListener("click",function(e){var r=menu.getBoundingClientRect();if(e.target===menu&&(e.clientX<r.left||e.clientX>r.right||e.clientY<r.top||e.clientY>r.bottom))menu.close();});menu.addEventListener("close",function(){accountButton.setAttribute("aria-expanded","false");accountButton.focus();});
    bar.append(accountButton,gear);root.append(style,bar,menu);draw();
  }
  function resolvedTheme(){
    var candidate=(!host||!host.isConnected)&&anchor();var header=(host&&host.closest("header"))||(candidate&&candidate.header),c=header&&getComputedStyle(header).color;
    var rgb=c&&c.match(/[\d.]+/g);
    if(rgb&&rgb.length>=3)return .2126*+rgb[0]+.7152*+rgb[1]+.0722*+rgb[2]>140?"dark":"light";
    return matchMedia("(prefers-color-scheme: dark)").matches?"dark":"light";
  }
  function report(mounted,detector){var theme=resolvedTheme();if(root){root.host.style.colorScheme=theme;}var value=mounted+":"+detector+":"+theme;if(value===lastReport)return;lastReport=value;if(mounted&&host){var h=host.closest("header").getBoundingClientRect(),r=host.getBoundingClientRect();call("dlog",{msg:"controls: safe horizontal header="+Math.round(h.width)+"x"+Math.round(h.height)+" controls="+Math.round(r.width)+"x"+Math.round(r.height)});}var t=window.__TAURI__;if(!t||!t.core){lastReport="";return;}t.core.invoke("report_integration",{mounted:mounted,detector:detector,theme:theme}).catch(function(){lastReport="";});}
  function horizontal(e){
    var r=e.getBoundingClientRect(),style=getComputedStyle(e);
    return r.width>=260 && r.height>=36 && r.height<=100 && r.width>r.height*2 && style.visibility!=="hidden" && style.display!=="none";
  }
  function anchor(){
    var side=document.getElementById("side")||document.getElementById("pane-side"),sideBox=side&&side.getBoundingClientRect();
    var candidates=Array.from(document.querySelectorAll("#app header,#app [role=banner]"))
      .filter(function(e){
        if(e.closest("#main")||!horizontal(e))return false;
        var r=e.getBoundingClientRect();
        // In current WhatsApp the conversation-list header is a sibling of #side.
        // Its navigation rail is ALSO a header. Never select by document order.
        if(sideBox && sideBox.width>0)return Math.abs(r.left-sideBox.left)<=24 && Math.abs(r.width-sideBox.width)<=48 && (side.contains(e)||(r.bottom<=sideBox.top+4 && sideBox.top-r.bottom<=100));
        return !!e.querySelector("h1,h2,[role=heading]");
      });
    var header=candidates.find(function(e){return !candidates.some(function(other){return other!==e&&e.contains(other);});});
    if(!header)return null;
    var width=header.getBoundingClientRect().width;
    // Use the existing horizontal action row, not an outer header whose child
    // already occupies all available width. Do not alter WhatsApp's CSS.
    var rows=Array.from(header.querySelectorAll("div,section")).slice(0,32).filter(function(e){
      var c=getComputedStyle(e),r=e.getBoundingClientRect();return horizontal(e)&&c.display==="flex"&&c.flexDirection==="row"&&r.width>=width-48&&!!e.querySelector("button,[role=button]");
    });
    var row=rows.find(function(e){return !rows.some(function(other){return other!==e&&e.contains(other);});});
    if(!row){var c=getComputedStyle(header);if(c.display!=="flex"||c.flexDirection!=="row")return null;row=header;}
    return {header:header,row:row};
  }
  function fits(target){
    var area=target.header.getBoundingClientRect(),r=host.getBoundingClientRect();
    if(r.width<84||r.height<36||r.left<area.left||r.right>area.right+1||r.top<area.top||r.bottom>area.bottom+1)return false;
    return !Array.from(target.header.querySelectorAll("button,[role=button],h1,h2,[role=heading]")).some(function(e){
      if(host.contains(e))return false;var b=e.getBoundingClientRect();return b.width>0&&b.height>0&&r.left<b.right-1&&r.right>b.left+1&&r.top<b.bottom-1&&r.bottom>b.top+1;
    });
  }
  function mount(){
    var target=anchor();if(!host)create();
    var resizeNode=document.getElementById("side")||document.documentElement;
    if(resizeObserver&&resizeTarget!==resizeNode){resizeObserver.disconnect();resizeObserver.observe(resizeNode);resizeTarget=resizeNode;}
    if(target){
      host.dataset.compact=String(target.header.getBoundingClientRect().width<560);
      if(host.parentNode!==target.row){if(menu.open)menu.close();target.row.appendChild(host);}
      if(!fits(target)&&host.dataset.compact!=="true")host.dataset.compact="true";
      if(!fits(target)){host.remove();target=null;}
    }else if(host.isConnected)host.remove();
    report(!!target,messageRows().length>0);
    scanMessages();
    if(menu.open)positionMenu();
    // Consume our own insertion/removal records after processing this layout.
    // Otherwise an unsuitable header causes append/remove retries every 200ms.
    if(observer)observer.takeRecords();
  }
  function schedule(){if(scheduled)return;scheduled=setTimeout(function(){scheduled=0;mount();},200);}
  // Adapter contract: only mounted message IDs/direction and explicit mute state.
  // No React internals, message text, chat names, or contacts are read.
  var cursor=null, chat=null, baseline=0, muteKnown=null, sent=new Set(), lastMenuText="", menuChat=null, menuTime=0;
  document.addEventListener("click",function(e){var item=e.target.closest && e.target.closest("[role=menuitem]");if(item){muteKnown=null;return;}var control=e.target.closest && e.target.closest("#main header button,#main header [role=button]");if(control){menuChat=chat;menuTime=Date.now();}},true);
  function messageRows(){var main=document.getElementById("main");return main?Array.from(main.querySelectorAll("[data-id]")).filter(function(e){return /^(true|false)_[^_]+_.+/.test(e.getAttribute("data-id")||"");}):[];}
  function muteState(){
    var main=document.getElementById("main");if(!main)return null;
    if(main.querySelector('header[data-muted="true"], header [data-muted="true"], header [data-icon="muted"], header [data-icon="muted-alt"]'))return true;
    if(main.querySelector('header[data-muted="false"], header [data-muted="false"]'))return false;
    // Only explicit, visible actions prove the opposite current state. These
    // bounded labels belong to this adapter, never to the mounting algorithm.
    var actions=Array.from(document.querySelectorAll('[role="menuitem"],[role="menu"] button,[role="menu"] [role="button"]')).filter(function(e){return e.getClientRects().length;});
    var text=actions.map(function(e){return e.textContent.trim().toLowerCase();}).join("|");
    if(menuChat===chat && Date.now()-menuTime<5000 && text){lastMenuText=text;if(actions.some(function(e){return /^(unmute notifications|reativar notificações|ativar notificações)$/.test(e.textContent.trim().toLowerCase());}))muteKnown=true;
      else if(actions.some(function(e){return /^(mute notifications|silenciar notificações)$/.test(e.textContent.trim().toLowerCase());}))muteKnown=false;}
    return muteKnown;
  }
  async function send(row,muted){
    var id=row.getAttribute("data-id");if(sent.has(id))return;sent.add(id);if(sent.size>1024)sent.delete(sent.values().next().value);
    if(!crypto.subtle)return;
    var bytes=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(id));
    var key=Array.from(new Uint8Array(bytes)).map(function(b){return b.toString(16).padStart(2,"0");}).join("");
    await call("report_message_signal",{signal:{id:key,active:true,muted:muted}});
  }
  function scanMessages(){
    var rows=messageRows();if(!rows.length){cursor=null;chat=null;return;}
    var end=rows[rows.length-1].getAttribute("data-id"), current=end.split("_")[1];
    if(current!==chat){chat=current;cursor=end;baseline=Date.now();muteKnown=null;lastMenuText="";rows.forEach(function(r){sent.add(r.getAttribute("data-id"));});return;}
    var muted=muteState();
    if(end===cursor)return;
    var index=rows.findIndex(function(r){return r.getAttribute("data-id")===cursor;});
    cursor=end;
    // Disappearing anchor = virtualization/reconnect/history, not proof of news.
    if(index<0 || Date.now()-baseline<2500)return;
    rows.slice(index+1).filter(function(r){return r.getAttribute("data-id").startsWith("false_");}).forEach(function(r){send(r,muted);});
  }
  function start(){
    if(!document.body)return;
    observer=new MutationObserver(function(records){if(records.some(function(r){return !host || !host.contains(r.target);}))schedule();});
    observer.observe(document.body,{subtree:true,childList:true,attributes:true,attributeFilter:["data-id","data-muted"]});
    if(window.ResizeObserver)resizeObserver=new ResizeObserver(schedule);
    themeObserver=new MutationObserver(schedule);
    themeObserver.observe(document.documentElement,{attributes:true,attributeFilter:["class","style","data-theme"]});
    themeObserver.observe(document.body,{attributes:true,attributeFilter:["class","style","data-theme"]});
    window.addEventListener("resize",mount);
    mount();call("integration_state").then(window.__whatrustState);
    setInterval(function(){call("integration_state").then(window.__whatrustState);mount();},10000);
    window.addEventListener("pagehide",function(){observer.disconnect();if(resizeObserver)resizeObserver.disconnect();themeObserver.disconnect();window.removeEventListener("resize",mount);});
  }
  if(document.body)start();else document.addEventListener("DOMContentLoaded",start,{once:true});
})();
