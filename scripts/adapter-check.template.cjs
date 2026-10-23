async (page) => {
  const checks=[];
  function check(name,pass){if(!pass)throw Error(name);checks.push(name);}
  await page.clock.install();
  await page.addInitScript(() => {
    window.__WHATRUST_WINDOWS__=true;window.calls=[];
    window.__TAURI__={core:{invoke:async(cmd,args)=>{window.calls.push({cmd,args});if(cmd==='integration_state')return {accounts:[{id:'default',name:'Matheus',unread:2}],current_id:'default',native_notifications:true};return null;}}};
  });
  await page.route('https://web.whatsapp.com/**',route=>route.fulfill({contentType:'text/html',body:'<html><meta charset="utf-8"><style>body{margin:0;background:#161717;color:#eee;font:14px system-ui}#app>header{display:flex;align-items:center;height:64px;padding:0 24px;gap:16px}h1{font-size:22px;flex:1}#main{padding:24px}</style><div id="app"><header><h1>WhatsApp</h1><button aria-label="Menu">⋮</button></header><div id="main"><header data-muted="false"></header><div id="messages"><div data-id="false_chatA_history">Histórico</div></div></div></div></html>'}));
  await page.goto('https://web.whatsapp.com/');
  await page.addScriptTag({content:__ADAPTER_SOURCE__});
  await page.clock.runFor(3000);
  const signals=()=>page.evaluate(()=>window.calls.filter(c=>c.cmd==='report_message_signal').map(c=>c.args.signal));
  check('initial_history_is_silent',(await signals()).length===0);
  async function append(id){await page.evaluate(id=>{const r=document.createElement('div');r.dataset.id=id;r.textContent='Mesmo texto';document.getElementById('messages').appendChild(r);},id);await page.clock.runFor(300);await page.waitForTimeout(20);}
  await append('false_chatA_new1');await append('false_chatA_new2');
  let s=await signals();check('identical_text_distinct_messages',s.length===2&&s[0].id!==s[1].id);
  check('identifier_is_hashed',s.every(x=>/^[a-f0-9]{64}$/.test(x.id)));
  check('explicit_unmuted_state',s.every(x=>x.muted===false));
  await append('true_chatA_outgoing');check('outgoing_is_silent',(await signals()).length===2);
  await page.evaluate(()=>document.querySelector('#main header').dataset.muted='true');await append('false_chatA_muted');
  check('muted_state_reported',(await signals()).at(-1).muted===true);
  await page.evaluate(()=>document.querySelector('#main header').removeAttribute('data-muted'));await append('false_chatA_unknown');
  check('unknown_mute_state_reported',(await signals()).at(-1).muted===null);
  let count=(await signals()).length;
  await page.evaluate(()=>{const r=document.createElement('div');r.dataset.id='false_chatA_older';document.getElementById('messages').prepend(r);});await page.clock.runFor(300);
  check('scroll_history_is_silent',(await signals()).length===count);
  await page.evaluate(()=>document.getElementById('messages').innerHTML='<div data-id="false_chatB_history">Outra conversa</div>');await page.clock.runFor(300);
  check('conversation_change_is_silent',(await signals()).length===count);
  await page.getByRole('button',{name:'Configurações do whatRust'}).click();
  check('settings_uses_limited_opener',await page.evaluate(()=>window.calls.some(c=>c.cmd==='open_app_panel')));
  await page.evaluate(()=>document.querySelector('#app>header').remove());await page.clock.runFor(300);
  check('missing_header_activates_recovery',await page.evaluate(()=>window.calls.some(c=>c.cmd==='report_integration'&&c.args.mounted===false)));
  await page.evaluate(()=>{const h=document.createElement('header');h.innerHTML='<h1>WhatsApp</h1>';document.getElementById('app').prepend(h);});await page.clock.runFor(300);
  check('header_replacement_remounts_once',await page.locator('#__whatrust_controls').count()===1);
  await page.getByRole('button',{name:'Trocar conta: Matheus · 2 conversas não lidas',exact:true}).click();
  await page.evaluate(()=>window.__whatrustState({accounts:[{id:'default',name:'Matheus',unread:3}],current_id:'default',native_notifications:true}));
  check('live_unread_update_preserves_menu_focus',await page.locator('#__whatrust_controls').evaluate(host=>host.shadowRoot.activeElement?.dataset.item==='default'));
  await page.keyboard.press('Escape');
  check('escape_restores_focus',await page.locator('#__whatrust_controls').evaluate(host=>host.shadowRoot.activeElement?.title==='Conta: Matheus — Trocar conta'));
  for(const width of [560,900,1920]){await page.setViewportSize({width,height:800});const box=await page.locator('#__whatrust_controls').boundingBox();check('header_fits_'+width,box.x>=0&&box.x+box.width<=width);}
  // Regression from the real WhatsApp DOM: the 64px navigation rail is
  // itself a header, before two nested list headers and a full-width action row.
  await page.evaluate(()=>{
    document.body.innerHTML=`<style>
      #app{display:flex;width:100vw;height:100vh}#app>#rail{box-sizing:border-box;width:64px;height:100vh;display:flex;align-items:center;flex-direction:column;flex:none}
      #list{width:clamp(340px,30vw,768px);flex:none}#outer,#inner{width:100%;height:64px;box-sizing:border-box}
      #inner{display:flex;align-items:center;padding:0 20px}#actions{display:flex;align-items:center;width:100%;height:40px}
      #actions h1{font:700 20px system-ui;flex:1;margin:0}#native{display:flex;flex:none}#native button{width:40px;height:40px}
      #side{width:100%;height:600px}#main{flex:1;padding:0}
      </style><div id="app"><header id="rail"><button>Chats</button><button>Status</button></header><section id="list"><header id="outer"><header id="inner"><div id="actions"><h1>WhatsApp</h1><span id="native"><button aria-label="Menu original">⋮</button><button aria-label="Nova conversa">+</button></span></div></header></header><div id="side"></div></section><div id="main"><header style="height:64px">Conversa</header></div></div>`;
  });await page.clock.runFor(300);
  check('rejects_vertical_navigation_header',await page.locator('#actions>#__whatrust_controls').count()===1);
  for(const width of [560,900,1920]){
    await page.setViewportSize({width,height:800});await page.waitForTimeout(30);await page.clock.runFor(300);
    const fit=await page.locator('#__whatrust_controls').evaluate(host=>{
      const r=host.getBoundingClientRect(),header=host.closest('header').getBoundingClientRect();
      const buttons=Array.from(host.shadowRoot.querySelectorAll('.bar>button')).map(e=>e.getBoundingClientRect());
      const native=Array.from(document.querySelectorAll('#native button')).map(e=>e.getBoundingClientRect());
      return r.left>=header.left&&r.right<=header.right+1&&r.bottom<=header.bottom+1&&buttons.every(b=>b.width>=40&&b.height>=40)&&native.every(b=>b.right<=r.left||b.left>=r.right);
    });if(!fit)throw Error('header geometry '+JSON.stringify(await page.locator('#__whatrust_controls').evaluate(h=>({host:h.getBoundingClientRect().toJSON(),head:h.closest('header').getBoundingClientRect().toJSON(),buttons:Array.from(h.shadowRoot.querySelectorAll('.bar>button')).map(e=>e.getBoundingClientRect().toJSON()),native:Array.from(document.querySelectorAll('#native button')).map(e=>e.getBoundingClientRect().toJSON())}))));check('realistic_header_controls_fit_'+width,fit);
  }
  await page.evaluate(()=>document.getElementById('list').style.width='180px');await page.waitForTimeout(40);await page.clock.runFor(300);
  check('unsafe_narrow_header_uses_recovery',await page.locator('#__whatrust_controls').count()===0&&await page.evaluate(()=>window.calls.filter(c=>c.cmd==='report_integration').at(-1).args.mounted===false));
  await page.evaluate(()=>document.getElementById('list').style.width='');await page.waitForTimeout(40);await page.clock.runFor(300);
  check('safe_width_restores_controls_once',await page.locator('#actions>#__whatrust_controls').count()===1);
  await page.evaluate(()=>{window.__mountMutations=0;window.__probe=new MutationObserver(rs=>{for(const r of rs)for(const n of [...r.addedNodes,...r.removedNodes])if(n.id==='__whatrust_controls')window.__mountMutations++;});window.__probe.observe(document.getElementById('app'),{subtree:true,childList:true});document.getElementById('list').style.width='280px';});
  await page.waitForTimeout(40);await page.clock.runFor(1600);
  check('unsafe_header_does_not_retry_own_mutations',await page.evaluate(()=>window.__mountMutations<=4));
  await page.evaluate(()=>{window.__probe.disconnect();document.getElementById('list').style.width='';});await page.waitForTimeout(40);await page.clock.runFor(300);
  check('reports_whatsapp_dark_theme',await page.evaluate(()=>window.calls.filter(c=>c.cmd==='report_integration').at(-1).args.theme==='dark'));
  await page.evaluate(()=>document.body.style.color='rgb(10,10,10)');await page.clock.runFor(300);
  check('reports_whatsapp_light_theme_change',await page.evaluate(()=>window.calls.filter(c=>c.cmd==='report_integration').at(-1).args.theme==='light'));
  return {passed:true,checks};
}