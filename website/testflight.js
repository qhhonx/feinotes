(() => {
  const section = document.querySelector('.beta-access');
  if (!section) return;
  const english = section.dataset.language === 'en';
  const blocks = [...section.querySelectorAll('[data-beta-platform]')];
  if (!blocks.length) blocks.push(section);
  const render = (block, status = {}) => {
    const platform = block.dataset.betaPlatform || 'MAC_OS';
    const mobile = platform === 'IOS';
    const link = block.querySelector('[data-beta-link]');
    const review = block.querySelector('[data-beta-review]');
    const prefix = mobile ? 'iPhone / iPad · ' : '';
    const pending = english ? 'Beta in review' : '测试版审核中';
    const unavailable = english ? 'Beta coming soon' : '测试版准备中';
    link.removeAttribute('href');
    link.setAttribute('aria-disabled', 'true');
    link.setAttribute('tabindex', '-1');
    link.textContent = prefix + (status.state === 'review' ? pending : unavailable);
    if (review) review.hidden = true;
    if (status.state !== 'available') return;
    try {
      const invitation = new URL(status.url);
      if (invitation.origin !== 'https://testflight.apple.com' || !/^\/join\/[A-Za-z0-9]+$/.test(invitation.pathname)) return;
      link.href = invitation.href;
      link.removeAttribute('aria-disabled');
      link.removeAttribute('tabindex');
      link.textContent = prefix + (english ? 'Try with TestFlight' : '通过 TestFlight 体验');
      if (review && status.pending?.state === 'review') {
        review.textContent = english
          ? `${status.pending.version} in review · ${status.version} available`
          : `${status.pending.version} 审核中 · 可体验 ${status.version}`;
        review.hidden = false;
      }
    } catch (_) { /* Keep an invalid invitation disabled. */ }
  };
  fetch(section.dataset.status, { cache: 'no-store' })
    .then(response => {
      if (!response.ok) throw new Error('Availability could not be loaded');
      return response.json();
    })
    .then(status => blocks.forEach(block => {
      const platform = block.dataset.betaPlatform || 'MAC_OS';
      render(block, status.platforms?.[platform] || (platform === 'MAC_OS' ? status : {}));
    }))
    .catch(() => blocks.forEach(block => render(block)));
})();
