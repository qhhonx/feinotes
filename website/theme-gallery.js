(() => {
  const buttons = [...document.querySelectorAll('.theme-switch button')];
  const scenes = [...document.querySelectorAll('.story[data-scene]')];

  for (const button of buttons) {
    button.addEventListener('click', () => {
      for (const scene of scenes) {
        scene.hidden = scene.dataset.scene !== button.dataset.theme;
      }
      for (const item of buttons) {
        const selected = item === button;
        item.classList.toggle('active', selected);
        item.setAttribute('aria-pressed', String(selected));
      }
    });
  }
})();
