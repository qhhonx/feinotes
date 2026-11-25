// Keep the invitation formats and iCloud address checks aligned with src/sharing/links.rs.
const WEB = 'https://feinotes.app/share/?';
const regions = {c:'https://www.icloud.com.cn/share/', g:'https://www.icloud.com/share/'};
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

export function invitationFromFragment(fragment) {
  if (!fragment.startsWith('#')) return null;
  try { return invitation(WEB + fragment.slice(1)).web; }
  catch { return null; }
}

export function cloudInvitationURL(value) {
  if (validAppleAddress(value)) return value;
  const prefix = value.startsWith(WEB) ? WEB : APP;
  if (!value.startsWith(prefix)) throw new Error('Invalid invitation');
  const query = value.slice(prefix.length);
  const legacy = /^([cg])=([A-Za-z0-9_-]{1,256})$/.exec(query);
  if (legacy) return regions[legacy[1]] + legacy[2];
  if (!query.startsWith(V2)) throw new Error('Invalid invitation');
  const token = query.slice(V2.length);
  if (token.length > Math.ceil(MAX * 4 / 3) || !/^[A-Za-z0-9_-]+$/.test(token)) throw new Error('Invalid invitation');
  const apple = atob(token.replaceAll('-', '+').replaceAll('_', '/'));
  if (encodeApple(apple) !== token || !validAppleAddress(apple)) throw new Error('Invalid invitation');
  return apple;
}

function encodeApple(apple) {
  return btoa(apple).replaceAll('+', '-').replaceAll('/', '_').replace(/=+$/, '');
}

export function invitationAppURL(value) {
  const apple = cloudInvitationURL(value);
  if (value.startsWith(WEB)) return APP + value.slice(WEB.length);
  if (value.startsWith(APP)) return value;
  return APP + V2 + encodeApple(apple);
}

export function invitation(location) {
  if (!location.startsWith(WEB)) throw new Error('Invalid invitation');
  return {web:location, app:invitationAppURL(location), browser:'/w/#' + location.slice(WEB.length)};
}
