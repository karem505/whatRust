"use strict";
const invoke=window.__TAURI__.core.invoke;
const select=document.getElementById("accounts"),measure=document.getElementById("account-size");
let busy=false,activeId=null;
function fitSelectedName(){const text=select.selectedOptions[0]?.textContent||"Contas";measure.textContent=text;select.title=text;}
window.refresh=async(currentId)=>{
  if(currentId!==undefined)activeId=currentId;
  if(busy)return;busy=true;
  try{
    const list=await invoke("list_accounts"),previous=select.value;
    select.replaceChildren();
    for(const a of list){const option=document.createElement("option");option.value=a.id;option.textContent=a.name+(a.unread?" · "+a.unread:"");select.appendChild(option);}
    const chosen=activeId||previous||list.find(a=>a.is_default)?.id;
    if(list.some(a=>a.id===chosen))select.value=chosen;
    fitSelectedName();
  }catch{}finally{busy=false;}
};
document.getElementById("settings").onclick=()=>invoke("open_app_panel");
select.onchange=()=>{fitSelectedName();invoke("open_account",{id:select.value}).catch(()=>refresh());};
refresh();
