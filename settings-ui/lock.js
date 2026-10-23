const invoke = window.__TAURI__.core.invoke;
const pwd = document.getElementById("password");
const errEl = document.getElementById("error");

let busy = false;

async function tryUnlock() {
  if (busy) return;
  const password = pwd.value;
  if (!password) { errEl.textContent = "Digite sua senha."; return; }
  busy = true;
  document.getElementById("unlock").disabled = true;
  document.getElementById("biometric").disabled = true;
  errEl.textContent = "";
  try {
    const ok = await invoke("unlock_password", { password });
    if (!ok) {
      errEl.textContent = "Senha incorreta. Tente novamente.";
      pwd.value = "";
      pwd.focus();
    }
  } catch (e) {
    const wait = /^throttled:(\d+)$/.exec(String(e));
    errEl.textContent = wait
      ? t("Muitas tentativas. Tente de novo em {0}s.", [wait[1]])
      : t("Não foi possível desbloquear. Tente novamente.");
  } finally {
    busy = false;
    document.getElementById("unlock").disabled = false;
    document.getElementById("biometric").disabled = false;
  }
}

async function tryBiometric() {
  if (busy) return;
  busy = true;
  document.getElementById("unlock").disabled = true;
  document.getElementById("biometric").disabled = true;
  errEl.textContent = "";
  try {
    const ok = await invoke("unlock_biometric");
    if (!ok) errEl.textContent = "Não foi possível confirmar a biometria. Digite sua senha.";
  } catch (e) {
    errEl.textContent = "Não foi possível desbloquear. Tente novamente.";
  } finally {
    busy = false;
    document.getElementById("unlock").disabled = false;
    document.getElementById("biometric").disabled = false;
  }
}

async function init() {
  document.getElementById("unlock").addEventListener("click", tryUnlock);
  pwd.addEventListener("keydown", (e) => { if (e.key === "Enter") tryUnlock(); });

  const bio = document.getElementById("biometric");
  bio.addEventListener("click", tryBiometric);

  document.getElementById("reset").addEventListener("click", async (e) => {
    e.preventDefault();
    const ok = await Dlg.confirm(
      "Isso desconecta todas as contas deste aplicativo e remove o bloqueio. Você precisará vincular o WhatsApp novamente pelo QR Code. Suas conversas continuam no celular.",
      { title: "Redefinir o whatRust?", okLabel: "Redefinir", danger: true }
    );
    if (!ok) return;
    try {
      await invoke("reset_app_lock", { confirmed: true });
    } catch (err) {
      errEl.textContent = "Não foi possível redefinir o aplicativo. Tente novamente.";
    }
  });

  try {
    const s = await invoke("get_lock_status");
    if (s.biometric_enabled) {
      bio.textContent = s.biometric_label ? t("Usar {0}", [s.biometric_label]) : t("Usar biometria");
      bio.hidden = false;
      tryBiometric(); // auto-prompt on load
    }
  } catch (_) {
    // status unavailable — password still works
  }
  pwd.focus();
}

window.addEventListener("DOMContentLoaded", init);
