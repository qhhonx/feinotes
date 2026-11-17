// Keep the direct region/identifier mapping aligned with src/sharing/links.rs.
export function invitation(location) {
  const url = new URL(location);
  if (url.protocol !== 'https:' || url.hostname !== 'feinotes.app' || url.port || url.username || url.password || url.pathname !== '/share/' || url.hash) throw new Error('Invalid invitation');
  const match = /^\?([cg])=([A-Za-z0-9_-]{1,256})$/.exec(url.search);
  if (!match) throw new Error('Invalid invitation');
  const [, region, id] = match;
  const path = `?${region}=${id}`;
  return {web:`https://feinotes.app/share/${path}`, app:`feinotes://share/${path}`};
}
