"use strict";
const invoke=window.__TAURI__.core.invoke;
let busy=false;
window.refresh=async()=>{if(busy)return;busy=true;try{const list=await invoke("list_accounts"),sel=document.getElementById("accounts"),previous=sel.value;sel.replaceChildren();for(const a of list){const o=document.createElement("option");o.value=a.id;o.textContent=a.name+(a.unread?" · "+a.unread:"");sel.appendChild(o);}if(list.some(a=>a.id===previous))sel.value=previous;}catch{}finally{busy=false;}};
document.getElementById("settings").onclick=()=>invoke("open_app_panel");
document.getElementById("accounts").onchange=e=>invoke("open_account",{id:e.target.value});
refresh();
