async(page)=>{
 const checks=[];const check=(name,ok)=>{if(!ok)throw Error(name);checks.push(name);};
 await page.addInitScript(()=>{
  window.__settings={close_to_tray:true,start_minimized:false,autostart:false,hotkey_enabled:true,hotkey:'CmdOrCtrl+Shift+W',notifications:true,notify_when_focused:true,notify_active_chat:true,respect_muted:true,zoom:1,menu_bar_icon:true};
  window.__accounts=[{id:'default',name:'Matheus',order:0,unread:2,is_default:true,detached:false},{id:'acct-1',name:'Empresa '+('NomeMuitoLongo'.repeat(18)),order:1,unread:0,is_default:false,detached:false}];
  window.__events=[];window.__TAURI__={core:{invoke:async(cmd,args)=>{window.__events.push({cmd,args});if(cmd==='list_accounts')return window.__accounts;if(cmd==='get_settings')return window.__settings;if(cmd==='set_settings'){window.__settings=args.settings;return null;}if(cmd==='get_lock_status')return {enabled:false,biometric_available:false};if(cmd==='notification_diagnostics')return {version:'test',accounts:window.__accounts,adapters:{},native:{},recent:[]};if(cmd==='unlock_password')return false;if(cmd==='unlock_biometric')return false;if(cmd==='test_notification')return 'Teste solicitado';return null;}}};
 });
 const url=__SETTINGS_URL__;
 await page.goto(url);
 const names=['Contas','Notificações','Aparência','Segurança','Inicialização','Atalhos','Diagnóstico'];
 for(const scheme of ['light','dark'])for(const [width,height] of [[560,480],[900,700],[1920,1000],[2560,1300]]){
  await page.emulateMedia({colorScheme:scheme});await page.setViewportSize({width,height});
  for(const name of names){await page.getByRole('link',{name,exact:true}).click();check('panel_'+scheme+'_'+width+'_'+name,await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth&&Array.from(document.querySelectorAll('.prefs input,.prefs select,.prefs button')).filter(e=>e.getClientRects().length).every(e=>{const r=e.getBoundingClientRect();return r.left>=0&&r.right<=innerWidth+1;})));}
  await page.getByRole('link',{name:'Contas',exact:true}).click();
  const trigger=page.getByRole('button',{name:'Opções de Matheus',exact:true});
  await trigger.hover();
  await page.waitForTimeout(160);
  check('account_menu_hover_'+scheme+'_'+width,await trigger.evaluate(e=>getComputedStyle(e).backgroundColor!=='rgba(0, 0, 0, 0)'));
  await trigger.click();
  await page.waitForFunction(()=>document.querySelector('[aria-label="Opções de Matheus"]').getAttribute('aria-expanded')==='true');
  check('account_menu_open_feedback_'+scheme+'_'+width,await trigger.evaluate(e=>getComputedStyle(e).backgroundColor!=='rgba(0, 0, 0, 0)'));
  check('menu_anchored_'+scheme+'_'+width,await page.evaluate(()=>{const b=document.querySelector('[aria-label="Opções de Matheus"]').getBoundingClientRect(),m=document.getElementById('account-actions').getBoundingClientRect();const gap=Math.min(Math.abs(m.top-b.bottom-6),Math.abs(b.top-m.bottom-6));return m.left>=12&&m.right<=innerWidth-12+1&&m.top>=12&&m.bottom<=innerHeight-12+1&&Math.abs(m.right-b.right)<=1&&gap<=1;}));
  await page.keyboard.press('End');check('menu_keyboard_'+scheme+'_'+width,await page.evaluate(()=>document.activeElement===document.querySelector('#account-actions button:last-child')));
  await page.keyboard.press('Escape');check('menu_focus_restore_'+scheme+'_'+width,await trigger.evaluate(e=>document.activeElement===e));
 }
 await page.setViewportSize({width:900,height:600});await page.getByRole('link',{name:'Contas',exact:true}).click();await page.getByRole('button',{name:'Opções de Matheus',exact:true}).click();
 await page.getByRole('menuitem',{name:'Renomear',exact:true}).click();
 check('rename_dialog_focus_not_stolen',await page.getByRole('dialog').locator('input').evaluate(e=>document.activeElement===e));
 await page.keyboard.press('Escape');
 for(const [width,height] of [[560,480],[900,480]]){
  await page.setViewportSize({width,height});
  await page.evaluate(()=>{window.__auditDialog=Dlg.confirm('Conteúdo '+('teste'.repeat(200)),{title:'Confirmação '+('nome'.repeat(40)),okLabel:'Confirmar operação'});});
  check('long_dialog_within_'+width,await page.getByRole('dialog').evaluate(e=>{const r=e.getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight&&e.scrollWidth<=e.clientWidth;}));
  await page.keyboard.press('Escape');
 }
 await page.setViewportSize({width:900,height:600});await page.getByRole('link',{name:'Contas',exact:true}).click();await page.getByRole('button',{name:'Opções de Matheus',exact:true}).click();
 await page.keyboard.press('Escape');await page.locator('.account-more').last().click();
 check('changing_menu_keeps_focus_in_new_menu',await page.evaluate(()=>document.getElementById('account-actions').contains(document.activeElement)));
 await page.setViewportSize({width:560,height:480});await page.waitForTimeout(50);
 check('menu_follows_resize',await page.evaluate(()=>{const b=document.querySelector('.account-more:last-child').getBoundingClientRect(),m=document.getElementById('account-actions').getBoundingClientRect();return m.left>=12&&m.right<=innerWidth-11&&m.top>=12&&m.bottom<=innerHeight-11;}));
 await page.keyboard.press('Escape');
 await page.evaluate(()=>{const f=document.createElement('div');f.id='audit-filler';f.style.height='1200px';document.getElementById('sec-contas').appendChild(f);});
 await page.getByRole('button',{name:'Opções de Matheus',exact:true}).click();
 await page.evaluate(()=>document.querySelector('.prefs').scrollTo({top:900,behavior:'instant'}));await page.waitForFunction(()=>document.getElementById('account-actions').dataset.open==='false');
 check('menu_closes_when_anchor_scrolls_away',await page.evaluate(()=>document.getElementById('account-actions').dataset.open==='false'&&document.querySelector('.prefs').scrollTop>500));
 await page.evaluate(()=>{document.getElementById('audit-filler').remove();document.querySelector('.prefs').scrollTop=0;window.__nativeShow=HTMLElement.prototype.showPopover;HTMLElement.prototype.showPopover=undefined;});
 await page.getByRole('button',{name:'Opções de Matheus',exact:true}).click();
 check('menu_fallback_anchored',await page.evaluate(()=>{const b=document.querySelector('[aria-label="Opções de Matheus"]').getBoundingClientRect(),m=document.getElementById('account-actions').getBoundingClientRect();return Math.abs(m.right-b.right)<=1&&Math.min(Math.abs(m.top-b.bottom-6),Math.abs(b.top-m.bottom-6))<=1;}));
 await page.keyboard.press('Escape');await page.evaluate(()=>HTMLElement.prototype.showPopover=window.__nativeShow);
 await page.evaluate(()=>{window.__oneButtonDialog=Dlg.alert('Teste de foco');});await page.keyboard.press('Tab');
 check('single_button_dialog_traps_focus',await page.getByRole('dialog').evaluate(e=>e.contains(document.activeElement)));await page.keyboard.press('Escape');
 await page.goto(new URL('lock.html',url).href);
 for(const [width,height] of [[560,480],[900,700]]){await page.setViewportSize({width,height});check('lock_controls_'+width,await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth&&Array.from(document.querySelectorAll('button,input')).filter(e=>e.getClientRects().length).every(e=>{const r=e.getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;})));}
 for(const scheme of ['light','dark'])for(const [width,height] of [[320,480],[420,540]]){
  await page.emulateMedia({colorScheme:scheme});await page.setViewportSize({width,height});
  check('lock_layout_'+scheme+'_'+width,await page.locator('.lock-card').evaluate(e=>{const r=e.getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;}));
  check('lock_vector_icon_'+scheme+'_'+width,await page.locator('.lock-glyph svg').count()===1&&!await page.locator('.lock-glyph').textContent());
  await page.screenshot({path:'output/playwright/lock-'+scheme+'-'+width+'-local5.png'});
 }
 await page.getByRole('button',{name:'Desbloquear',exact:true}).click();
 check('empty_password_portuguese',await page.locator('#error').textContent()==='Digite sua senha.');
 await page.getByLabel('Senha',{exact:true}).fill('incorreta');await page.keyboard.press('Enter');
 await page.waitForFunction(()=>document.getElementById('error').textContent.includes('Senha incorreta'));
 check('wrong_password_feedback_and_focus',await page.locator('#password').evaluate(e=>!e.value&&document.activeElement===e&&!document.getElementById('unlock').disabled));
 await page.getByRole('link',{name:'Esqueceu a senha? Redefinir…'}).click();
 check('reset_confirmation_portuguese',await page.getByRole('dialog').textContent().then(t=>t.includes('Redefinir o whatRust?')&&t.includes('todas as contas')&&t.includes('Cancelar')));
 await page.getByRole('button',{name:'Cancelar',exact:true}).click();
 check('cancel_reset_has_no_side_effect',await page.evaluate(()=>!window.__events.some(e=>e.cmd==='reset_app_lock')));
 return {passed:true,count:checks.length,checks};
}
