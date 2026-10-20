async(page)=>{
 const checks=[];const check=(n,p)=>{if(!p)throw Error(n);checks.push(n)};
 await page.addInitScript(()=>{window.calls=[];window.__TAURI__={core:{invoke:async(cmd,args)=>{window.calls.push({cmd,args});return cmd==='list_accounts'?[{id:'default',name:'Matheus',unread:2},{id:'acct-1',name:'VS Prestadora de Serviços',unread:0}]:null;}}};});
 await page.emulateMedia({colorScheme:'dark'});
 await page.goto(__SHELL_URL__);
 for(const width of [480,560,1920]){
  await page.setViewportSize({width,height:48});
  check('compact_shell_no_scroll_'+width,await page.evaluate(()=>document.documentElement.scrollHeight<=innerHeight&&document.documentElement.scrollWidth<=innerWidth&&getComputedStyle(document.querySelector('main')).display==='none'));
  check('toolbar_controls_fit_'+width,await page.evaluate(()=>Array.from(document.querySelectorAll('button,select')).every(e=>{const r=e.getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;})));
 }
 await page.selectOption('#accounts','acct-1');
 check('account_switch_preserved',await page.evaluate(()=>window.calls.some(c=>c.cmd==='open_account'&&c.args.id==='acct-1')));
 await page.getByRole('button',{name:'Configurações do whatRust',exact:true}).click();
 check('settings_access_preserved',await page.evaluate(()=>window.calls.some(c=>c.cmd==='open_app_panel')));
 await page.setViewportSize({width:1000,height:800});
 check('full_window_empty_state',await page.getByRole('heading',{name:'Escolha uma conta para continuar'}).isVisible());
 check('neutral_dark_background',await page.evaluate(()=>getComputedStyle(document.body).backgroundColor==='rgb(22, 23, 23)'));
 await page.setViewportSize({width:1280,height:48});

 await page.evaluate(()=>window.refresh('default'));
 check('arrow_close_to_short_name',await page.evaluate(()=>{const m=document.getElementById('account-size'),r=document.createRange();r.selectNodeContents(m);const arrow=document.querySelector('.chevron').getBoundingClientRect();return Math.abs(arrow.left-r.getBoundingClientRect().right-8)<2;}));
 await page.screenshot({path:'output/playwright/shell-short-local5.png'});
 const longName='Conta empresarial com um nome muito longo para conferir as reticências no seletor';
 await page.evaluate(name=>{const s=document.getElementById('accounts');s.options[1].textContent=name;s.value='acct-1';s.dispatchEvent(new Event('change'));},longName);
 const longWidth=await page.locator('#accounts').evaluate(e=>e.getBoundingClientRect().width);
 check('long_name_capped_with_ellipsis',await page.locator('#accounts').evaluate(e=>getComputedStyle(e).textOverflow==='ellipsis'&&e.getBoundingClientRect().width<=320&&document.getElementById('account-size').scrollWidth>e.clientWidth));
 check('full_name_available_in_tooltip',await page.locator('#accounts').getAttribute('title')===longName);
 await page.screenshot({path:'output/playwright/shell-long-local5.png'});
 await page.evaluate(()=>window.refresh('acct-1'));
 check('selected_account_tracks_active_window',await page.locator('#accounts').inputValue()==='acct-1');
 await page.evaluate(()=>window.refresh('default'));
 check('short_name_shrinks_after_long_name',await page.locator('#accounts').evaluate(e=>e.getBoundingClientRect().width)<longWidth);

 await page.addInitScript(()=>{window.__TAURI__.core.invoke=(cmd)=>cmd==='list_accounts'?new Promise(resolve=>window.__releaseList=resolve):Promise.resolve();});
 await page.reload();
 await page.evaluate(async()=>{window.refresh('default');window.refresh('acct-1');window.__releaseList([{id:'default',name:'Matheus'},{id:'acct-1',name:'Empresa'}]);});
 await page.waitForFunction(()=>document.getElementById('accounts').options.length===2);
 check('delayed_refresh_keeps_latest_active_account',await page.locator('#accounts').inputValue()==='acct-1');
 return {passed:true,checks};
}
