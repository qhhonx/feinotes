import {invitation} from './links.mjs?v=share3';
const english = !navigator.language.toLowerCase().startsWith('zh');
if (english) {
  document.documentElement.lang = 'en';
  document.title = 'Shared document invitation · Fei Notes';
  document.querySelectorAll('[data-en]').forEach(el => { el.textContent = el.dataset.en; });
}
const status = document.getElementById('invite-status');
try {
  const link = invitation(location.href);
  document.getElementById('open-app').href = link.app;
  document.getElementById('invite-open').hidden = false;
  document.getElementById('invite-options').hidden = false;
  const opening = () => { status.textContent = english ? 'Opening Fei Notes…' : '正在打开飞记…'; };
  opening();
  document.getElementById('open-app').addEventListener('click', opening);
  document.getElementById('copy-link').addEventListener('click', async event => {
    event.preventDefault();
    try {
      await navigator.clipboard.writeText(link.web);
      status.textContent = english ? 'Invitation link copied.' : '已复制邀请链接。';
    } catch {
      status.textContent = english ? 'Copy the full address from your browser’s address bar.' : '请从浏览器地址栏复制完整地址。';
    }
  });
  // Try once per page load. Browser confirmation may still require a click.
  // Returning from the app never triggers another attempt.
  setTimeout(() => {
    if (document.visibilityState !== 'visible') return;
    try { location.assign(link.app); } catch { /* Keep the manual button available. */ }
  }, 0);
  setTimeout(() => {
    if (document.visibilityState === 'visible') status.textContent = english ? 'If Fei Notes did not open, use the button below.' : '若未打开，请点击下方按钮。';
  }, 1800);
} catch {
  status.textContent = english ? 'This invitation link is invalid. Ask the sender for a new link.' : '邀请链接无效，请向邀请者索取新的链接。';
}
