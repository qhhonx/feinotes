// Keep the invitation formats and iCloud address checks aligned with src/sharing/links.rs.
const WEB = 'https://feinotes.app/share/?';
const APP = 'feinotes://share/?';
const V2 = 'v=2&u=';
const MAX = 4096;
const hosts = ['https://www.icloud.com/share/', 'https://icloud.com/share/',
  'https://www.icloud.com.cn/share/', 'https://icloud.com.cn/share/'];

function validAppleAddress(value) {
  if (value.length > MAX || /[^\x21-\x7E]|\\/.test(value)) return false;
  const host = hosts.find(prefix => value.startsWith(prefix));
  if (!host) return false;
  const path = value.slice(host.length).split(/[?#]/, 1)[0];
  return path.length > 0 && !path.startsWith('/') && path.split('/').every(part => {
    const lower = part.toLowerCase();
    const dots = lower.replaceAll('%2e', '.');
    return !lower.includes('%2f') && !lower.includes('%5c') && dots !== '.' && dots !== '..';
  });
}

export function invitation(location) {
  if (!location.startsWith(WEB)) throw new Error('Invalid invitation');
  const query = location.slice(WEB.length);
  if (/^[cg]=[A-Za-z0-9_-]{1,256}$/.test(query)) return {web: location, app: APP + query};
  if (!query.startsWith(V2)) throw new Error('Invalid invitation');
  const token = query.slice(V2.length);
  if (token.length > Math.ceil(MAX * 4 / 3) || !/^[A-Za-z0-9_-]+$/.test(token)) throw new Error('Invalid invitation');
  const apple = atob(token.replaceAll('-', '+').replaceAll('_', '/'));
  const encoded = btoa(apple).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '');
  if (encoded !== token || !validAppleAddress(apple)) throw new Error('Invalid invitation');
  return {web: location, app: APP + query};
}
