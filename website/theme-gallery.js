(() => {
  const buttons = [...document.querySelectorAll('.theme-switch button')];
  const scenes = [...document.querySelectorAll('.story[data-scene]')];
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');
  const english = document.documentElement.lang === 'en';
  const controllers = scenes.map(scene => {
    if (scene.dataset.capture === "still") return { refresh() {} };
    const toggle = scene.querySelector('[data-animation-toggle]');
    const collaboration = scene.dataset.scene === 'collab';
    let frame = 0, timer = 0, run = 0;
    let visible = !('IntersectionObserver' in window);
    let paused = false;
    const eligible = () => !scene.hidden && visible && !paused && !reduced.matches && !document.hidden;
    const finish = () => {
      run++;
      cancelAnimationFrame(frame);
      clearTimeout(timer);
      scene.classList.remove('is-writing');
      scene.dataset.phase = 'complete';
    };
    const progress = (elapsed, delay, duration, steps) => Math.floor(Math.min(1, Math.max(0, (elapsed - delay) / duration)) * steps) / steps;
    const paint = elapsed => {
      if (collaboration) {
        scene.style.setProperty('--first-progress', progress(elapsed, 650, 2100, english ? 20 : 8));
        scene.style.setProperty('--second-progress', progress(elapsed, 1500, 1500, english ? 11 : 3));
      } else {
        scene.style.setProperty('--phone-progress', progress(elapsed, 650, 1800, english ? 27 : 9));
        scene.style.setProperty('--mac-progress', progress(elapsed, 1200, 1800, english ? 27 : 9));
      }
      scene.dataset.phase = elapsed < 650 ? 'ready' : 'writing';
    };
    const play = async () => {
      finish();
      if (!eligible()) return;
      const request = run;
      // All reveals use the decoded native capture, including collaborator labels.
      const ready = await Promise.allSettled([...scene.querySelectorAll('img')].map(img => img.decode()));
      if (request !== run || !eligible() || ready.some(item => item.status === 'rejected')) return;
      paint(0);
      scene.classList.add('is-writing');
      const started = performance.now();
      const tick = now => {
        if (request !== run || !eligible()) return;
        const elapsed = now - started;
        paint(elapsed);
        if (elapsed < 3600) frame = requestAnimationFrame(tick);
        else {
          finish();
          timer = setTimeout(play, 4000);
        }
      };
      frame = requestAnimationFrame(tick);
    };
    toggle?.addEventListener('click', () => {
      paused = !paused;
      const label = english ? (paused ? 'Play animation' : 'Pause animation') : (paused ? '播放动画' : '暂停动画');
      toggle.textContent = `${paused ? '▷' : 'Ⅱ'} ${label}`;
      toggle.setAttribute('aria-label', label);
      if (paused) finish(); else play();
    });
    if ('IntersectionObserver' in window) {
      new IntersectionObserver(entries => {
        const wasVisible = visible;
        visible = entries.some(entry => entry.isIntersecting && entry.intersectionRatio >= .35);
        if (visible && !wasVisible) play();
        else if (!visible) finish();
      }, { threshold: .35 }).observe(scene);
    } else play();
    return { refresh: () => { if (eligible()) play(); else finish(); } };
  });
  const refresh = () => controllers.forEach(controller => controller.refresh());
  for (const button of buttons) {
    button.addEventListener('click', () => {
      for (const scene of scenes) scene.hidden = scene.dataset.scene !== button.dataset.theme;
      for (const item of buttons) {
        const selected = item === button;
        item.classList.toggle('active', selected);
        item.setAttribute('aria-pressed', String(selected));
      }
      refresh();
    });
  }
  reduced.addEventListener('change', refresh);
  document.addEventListener('visibilitychange', refresh);
})();
