(() => {
  const section = document.querySelector('.beta-access');
  if (!section) return;
  const english = section.dataset.language === 'en';
  const link = section.querySelector('[data-beta-link]');
  const review = section.querySelector('[data-beta-review]');
  const pending = english ? 'Beta in review' : '测试版审核中';
  const unavailable = english ? 'Beta coming soon' : '测试版准备中';
  fetch(section.dataset.status, { cache: 'no-store' })
    .then(response => {
      if (!response.ok) throw new Error('Availability could not be loaded');
      return response.json();
    })
    .then(status => {
      if (status.state !== 'available') {
        link.textContent = status.state === 'review' ? pending : unavailable;
        return;
      }
      const invitation = new URL(status.url);
      if (invitation.origin !== 'https://testflight.apple.com' || !/^\/join\/[A-Za-z0-9]+$/.test(invitation.pathname)) {
        throw new Error('Invalid TestFlight invitation');
      }
      link.href = invitation.href;
      link.removeAttribute('aria-disabled');
      link.removeAttribute('tabindex');
      link.textContent = english ? 'Try with TestFlight' : '通过 TestFlight 体验';
      if (review && status.pending?.state === 'review') {
        review.textContent = english
          ? `${status.pending.version} in review · ${status.version} available`
          : `${status.pending.version} 审核中 · 可体验 ${status.version}`;
        review.hidden = false;
      }
    })
    .catch(() => { link.textContent = unavailable; });
})();
