(() => {
  const section = document.querySelector('.beta-access');
  if (!section) return;
  const english = section.dataset.language === 'en';
  const actions = section.querySelector('.beta-actions');
  const offers = [...actions.querySelectorAll('[data-beta-offer]')];
  const notice = section.querySelector('[data-beta-status]');
  const tryBeta = english ? 'Try with TestFlight' : '通过 TestFlight 体验';
  const publicBeta = english ? 'Public beta' : '公开测试版';
  const review = english ? 'Beta in review' : '测试版审核中';
  const unavailable = english ? 'Beta unavailable' : '测试版暂未开放';

  const invitationFor = status => {
    if (status.state !== 'available' || typeof status.url !== 'string') return null;
    try {
      const url = new URL(status.url);
      if (url.origin === 'https://testflight.apple.com' &&
          /^\/join\/[A-Za-z0-9]+$/.test(url.pathname) && !url.username && !url.password) {
        return url.href;
      }
    } catch { /* An invalid invitation cannot be used. */ }
    return null;
  };

  const versionOf = ({ version }) => typeof version === 'string' ? version.trim() : '';
  const versionRow = (label, version) => {
    const row = document.createElement('span');
    row.className = 'beta-version-row';
    if (label) {
      const platforms = document.createElement('span');
      platforms.className = 'beta-platform-names';
      platforms.textContent = label;
      row.append(platforms);
    }
    if (version) {
      const current = document.createElement('span');
      current.className = 'beta-current-version';
      current.textContent = `${publicBeta} ${version}`;
      if (label) row.append(' ');
      row.append(current);
    }
    return row;
  };
  const inReview = version => {
    if (!version) return review;
    return english ? `${version} in review` : `${version} 审核中`;
  };

  const render = availability => {
    const platforms = [
      { label: 'Mac', status: availability.platforms?.MAC_OS ?? availability },
      { label: 'iPhone / iPad', status: availability.platforms?.IOS ?? {} },
    ].map(platform => ({
      ...platform,
      invitation: invitationFor(platform.status),
      version: versionOf(platform.status),
    }));
    const [mac, mobile] = platforms;
    const shared = mac.invitation && mac.invitation === mobile.invitation;
    const invitations = shared ? [mac] : platforms.filter(platform => platform.invitation);
    actions.hidden = invitations.length === 0;
    offers.forEach((offer, index) => {
      const entry = invitations[index];
      offer.hidden = !entry;
      if (!entry) return;
      const link = offer.querySelector('[data-beta-link]');
      link.href = entry.invitation;
      link.textContent = shared ? tryBeta : `${tryBeta} · ${entry.label}`;
      const details = offer.querySelector('[data-beta-version]');
      if (shared && mac.version === mobile.version) {
        details.replaceChildren(versionRow('Mac · iPhone · iPad', mac.version));
      } else if (shared) {
        details.replaceChildren(...platforms.map(({ label, version }) => versionRow(label, version)));
      } else if (entry.version) {
        details.replaceChildren(versionRow('', entry.version));
      }
    });

    const messages = [];
    for (const { label, status, invitation, version } of platforms) {
      if (!invitation) {
        messages.push(`${label} · ${status.state === 'review' ? inReview(version) : unavailable}`);
      } else if (status.pending?.state === 'review') {
        const pending = versionOf(status.pending);
        const detail = pending && pending !== version
          ? inReview(pending)
          : (english ? 'New build in review' : '新构建审核中');
        messages.push(`${label} · ${detail}`);
      }
    }
    notice.textContent = messages.join('\n');
  };

  fetch(section.dataset.status, { cache: 'no-store' })
    .then(response => {
      if (!response.ok) throw new Error('Availability could not be loaded');
      return response.json();
    })
    .then(render)
    .catch(() => {
      notice.textContent = english ? 'Could not check beta availability.' : '暂时无法获取测试版状态。';
    });
})();
