(() => {
  const section = document.querySelector('.beta-access');
  if (!section) return;
  const english = section.dataset.language === 'en';
  const link = section.querySelector('[data-beta-link]');
  const label = section.querySelector('[data-beta-status]');
  const pending = english ? 'Waiting for Apple’s TestFlight review.' : '正在等待 Apple 的 TestFlight 测试版审核。';
  const unavailable = english ? 'The next TestFlight build is being prepared.' : '正在准备下一份 TestFlight 测试版。';
  fetch(section.dataset.status, { cache: 'no-store' })
    .then(response => {
      if (!response.ok) throw new Error('Availability could not be loaded');
      return response.json();
    })
    .then(status => {
      if (status.state !== 'available') {
        label.textContent = status.state === 'review' ? pending : unavailable;
        return;
      }
      const invitation = new URL(status.url);
      if (invitation.origin !== 'https://testflight.apple.com' || !/^\/join\/[A-Za-z0-9]+$/.test(invitation.pathname)) {
        throw new Error('Invalid TestFlight invitation');
      }
      link.href = invitation.href;
      link.removeAttribute('aria-disabled');
      link.removeAttribute('tabindex');
      label.textContent = english
        ? `Version ${status.version} · Build ${status.build} · Opens Apple TestFlight`
        : `版本 ${status.version} · 构建 ${status.build} · 使用 Apple TestFlight 安装`;
    })
    .catch(() => { label.textContent = unavailable; });
})();
