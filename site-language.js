// Keep links to the former combined Chinese and English pages working.
if (/^#en(?:-|$)/.test(location.hash)) {
  const english = document.querySelector('link[rel="alternate"][hreflang="en"]');
  if (english) {
    const destination = new URL(english.href);
    destination.hash = location.hash;
    location.replace(destination);
  }
}
