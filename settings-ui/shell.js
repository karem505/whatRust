"use strict";
const invoke=window.__TAURI__.core.invoke;
const select=document.getElementById("accounts"),measure=document.getElementById("account-size"),cards=document.getElementById("cards");
let busy=false,activeId=null;
function fitSelectedName(){const text=select.selectedOptions[0]?.textContent||"Contas";measure.textContent=text;select.title=text;}
function el(tag,cls,text){const e=document.createElement(tag);if(cls)e.className=cls;if(text!==undefined)e.textContent=text;return e;}
function plusIcon(){const svg=document.createElementNS("http://www.w3.org/2000/svg","svg"),path=document.createElementNS("http://www.w3.org/2000/svg","path");svg.setAttribute("viewBox","0 0 24 24");svg.setAttribute("aria-hidden","true");path.setAttribute("d","M12 5v14 M5 12h14");svg.appendChild(path);return svg;}
// One card per account (photo or initial, name, unread badge) plus an "add" card
// that opens Settings, where accounts are created. The grid wraps.
function drawCards(list,current){
  cards.replaceChildren();
  for(const a of Profiles.sorted(list)){
    const li=el("li"),b=el("button","card");b.type="button";b.dataset.id=a.id;
    if(a.id===current)b.setAttribute("aria-current","true");
    const avatar=el("span","avatar");
    if(a.photo){const img=el("img");img.src=a.photo;img.alt="";avatar.appendChild(img);}else avatar.textContent=Profiles.initial(a.name);
    b.append(avatar,el("span","card-name",a.name));
    if(a.is_default)b.appendChild(el("span","card-sub","Conta inicial"));
    const count=Profiles.badgeText(a.unread);
    if(count){const badge=el("span","count",count);badge.setAttribute("aria-hidden","true");b.appendChild(badge);}
    b.setAttribute("aria-label",a.name+(count?" · "+a.unread+" "+t("conversas não lidas"):""));
    b.onclick=()=>invoke("open_account",{id:a.id}).catch(()=>refresh());
    li.appendChild(b);cards.appendChild(li);
  }
  const li=el("li"),add=el("button","card add");add.type="button";
  const avatar=el("span","avatar");avatar.appendChild(plusIcon());
  add.append(avatar,el("span","card-name","Adicionar conta"));
  add.onclick=()=>invoke("open_app_panel");
  li.appendChild(add);cards.appendChild(li);
}
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
    drawCards(list,activeId);
  }catch{}finally{busy=false;}
};
document.getElementById("settings").onclick=()=>invoke("open_app_panel");
// The logo opens the same account menu as the switcher next to it (issue #128).
document.getElementById("logo").onclick=()=>{select.focus();try{select.showPicker();}catch{}};
select.onchange=()=>{fitSelectedName();invoke("open_account",{id:select.value}).catch(()=>refresh());};
refresh();
