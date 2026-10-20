(function () {
  "use strict";
  if (!window.__WHATRUST_WINDOWS__ || location.origin !== "https://web.whatsapp.com") return;
  var state = { accounts: [], current_id: "", native_notifications: false };
  var host, root, menu, accountButton, status, lastReport = "", observer, scheduled;
  function call(cmd, args) { var t=window.__TAURI__;if(!t||!t.core)return Promise.resolve(null);return t.core.invoke(cmd, args).catch(function () { return null; }); }
  function button(text, fn) { var b=document.createElement("button");b.type="button";b.textContent=text;b.addEventListener("click",function(e){e.stopPropagation();fn();});return b; }
  function openPanel() { if(menu.open)menu.close();call("open_app_panel"); }
  function draw() {
    if(!accountButton)return;
    var focusKey=root.activeElement && root.activeElement.dataset.item;
    var keepFocus=menu.open && !!focusKey;
    var own=state.accounts.find(function(a){return a.id===state.current_id;});
    accountButton.textContent=own ? own.name + (own.unread ? " · "+own.unread : "") : "Contas";
    accountButton.title="Trocar conta do WhatsApp";
    menu.replaceChildren();
    var title=document.createElement("h2");title.textContent="Suas contas";menu.appendChild(title);
    state.accounts.forEach(function(a){var b=button(a.name+(a.unread?" · "+a.unread+" conversas não lidas":""),function(){menu.close();call("switch_account",{id:a.id});});b.setAttribute("aria-current",String(a.id===state.current_id));b.dataset.item=a.id;menu.appendChild(b);});
    var manage=button("Gerenciar contas e configurações",openPanel);manage.dataset.item="manage";menu.appendChild(manage);
    var close=button("Fechar",function(){menu.close();});close.dataset.item="close";menu.appendChild(close);
    status=document.createElement("p");status.setAttribute("role","status");status.textContent="Controles do whatRust";menu.appendChild(status);
    if(keepFocus){var restore=Array.from(menu.querySelectorAll("button")).find(function(b){return b.dataset.item===focusKey;})||menu.querySelector("button");if(restore)restore.focus();}
  }
  window.__whatrustState=function(next){if(!next)return;state=next;window.__whatrustPreferNative=!!next.native_notifications;draw();};
  function create(){
    host=document.createElement("span");host.id="__whatrust_controls";host.setAttribute("data-whatrust","controls");
    host.style.cssText="display:inline-flex;align-items:center;flex:0 1 auto;min-width:40px;max-width:190px;margin-inline-start:8px";
    root=host.attachShadow({mode:"open"});
    var style=document.createElement("style");style.textContent=":host{color:inherit;font:13px system-ui}*{box-sizing:border-box}button{font:inherit;cursor:pointer;border:0;border-radius:8px;padding:7px 8px;background:transparent;color:inherit;min-height:36px}button:hover{background:#00806933}button:focus-visible{outline:2px solid #00a884;outline-offset:2px}.bar{display:flex;align-items:center;gap:5px;min-width:0}.account{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:110px;font-weight:500}svg{width:21px;height:21px;stroke:currentColor;fill:none;stroke-width:1.7;stroke-linecap:round;stroke-linejoin:round}@media(max-width:720px){.account{max-width:68px}}dialog{color-scheme:light dark;color:light-dark(#182229,#e9edef);background:light-dark(#fff,#202c33);border:1px solid #8884;border-radius:16px;box-shadow:0 12px 50px #0006;width:330px;max-width:calc(100vw - 32px);padding:16px}dialog::backdrop{background:#0004}dialog button{display:block;width:100%;text-align:left;margin:5px 0;border-radius:9px}h2{font-size:17px;margin:0 0 12px}p{font-size:12px;opacity:.7;margin:12px 0 0}[aria-current=true]{border-color:#00a884}";
    var bar=document.createElement("span");bar.className="bar";
    accountButton=button("Contas",function(){draw();if(!menu.open)menu.showModal();accountButton.setAttribute("aria-expanded","true");var b=menu.querySelector("button");if(b)b.focus();});accountButton.className="account";accountButton.setAttribute("aria-haspopup","dialog");accountButton.setAttribute("aria-expanded","false");
    var gear=button("⚙",openPanel);gear.setAttribute("aria-label","Configurações do whatRust");gear.title="Configurações do whatRust";
    gear.textContent="";
    var svg=document.createElementNS("http://www.w3.org/2000/svg","svg"),path=document.createElementNS("http://www.w3.org/2000/svg","path");svg.setAttribute("viewBox","0 0 24 24");svg.setAttribute("aria-hidden","true");path.setAttribute("d","M9 3h6l1 3l3 1l2 5l-2 5l-3 1l-1 3H9l-1-3l-3-1l-2-5l2-5l3-1z M15 12a3 3 0 1 1-6 0a3 3 0 0 1 6 0");svg.appendChild(path);gear.appendChild(svg);
    menu=document.createElement("dialog");menu.setAttribute("aria-label","Contas do whatRust");menu.addEventListener("close",function(){accountButton.setAttribute("aria-expanded","false");accountButton.focus();});
    bar.append(accountButton,gear);root.append(style,bar,menu);draw();
  }
  function report(mounted,detector){var value=mounted+":"+detector;if(value===lastReport)return;lastReport=value;var t=window.__TAURI__;if(!t||!t.core){lastReport="";return;}t.core.invoke("report_integration",{mounted:mounted,detector:detector}).catch(function(){lastReport="";});}
  function anchor(){return document.querySelector("#side header") || Array.from(document.querySelectorAll("#app header, #app [role=banner]")).find(function(e){return !e.closest("#main");}) || null;}
  function mount(){
    var target=anchor();if(!host)create();
    if(target && host.parentNode!==target)target.appendChild(host);
    if(!target && host.isConnected)host.remove();
    report(!!target,messageRows().length>0);
    scanMessages();
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
    mount();call("integration_state").then(window.__whatrustState);
    setInterval(function(){call("integration_state").then(window.__whatrustState);mount();},10000);
    window.addEventListener("pagehide",function(){observer.disconnect();});
  }
  if(document.body)start();else document.addEventListener("DOMContentLoaded",start,{once:true});
})();
