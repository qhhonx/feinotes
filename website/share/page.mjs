import {invitation} from './links.mjs?v=share7';
const english = !navigator.language.toLowerCase().startsWith('zh');
if (english) {
  document.documentElement.lang = 'en';
  document.title = 'Sharing invitation · Fei Notes';
  document.getElementById('get-app').href = '/en/#testflight';
  document.querySelector('.invite-brand').href = '/en/';
  document.querySelectorAll('[data-en]').forEach(el => { el.textContent = el.dataset.en; });
}
const status = document.getElementById('invite-status');
try {
  const link = invitation(location.href);
  document.getElementById('open-app').href = link.app;
  const openWeb = document.getElementById('open-web');
  if (!openWeb.hidden) openWeb.href = link.browser;
  document.getElementById('invite-open').hidden = false;
  document.getElementById('invite-options').hidden = false;
  const openingText = english ? 'Opening Fei Notes…' : '正在打开飞记…';
  let openingTimer;
  const opening = () => {
    status.textContent = openingText;
    clearTimeout(openingTimer);
    openingTimer = setTimeout(() => {
      if (status.textContent === openingText)
        status.textContent = english ? 'Open in the app to join for the first time.' : '首次加入共享，请用飞记 App 打开。';
    }, 1800);
  };
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
} catch {
  status.textContent = english ? 'This invitation link is invalid. Ask the sender for a new link.' : '邀请链接无效，请向邀请者索取新的链接。';
}
