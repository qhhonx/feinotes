// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Copyright 2026 Fei Notes contributors. See LICENSES.md.

export const prefix = 'https://feinotes.app/o/?id=';
const encoder = new TextEncoder();
const domain = encoder.encode('feinotes-object-scope-v1\0');
const uuidPattern = /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/;
const digest = async bytes => new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
const hex = bytes => Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
const uuidBytes = id => {
  if (!uuidPattern.test(id)) throw new Error('Invalid object ID');
  return Uint8Array.from(id.replaceAll('-', '').match(/../g), pair => parseInt(pair, 16));
};
const uuidString = bytes => hex(bytes).replace(/^(.{8})(.{4})(.{4})(.{4})(.{12})$/, '$1-$2-$3-$4-$5');

function concat(...parts) {
  const bytes = new Uint8Array(parts.reduce((length, part) => length + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    bytes.set(part, offset);
    offset += part.length;
  }
  return bytes;
}

async function scope(kind, owner, suffix) {
  const ownerKey = hex(await digest(encoder.encode(owner)));
  return (await digest(concat(domain, [kind], encoder.encode(ownerKey), suffix))).slice(0, 16);
}

export function personalScope(owner, zoneName) {
  const zone = /^QuietLibraryEdits-v4(?:-QA-([a-f0-9-]+))?$/.exec(zoneName);
  if (!zone || !owner) throw new Error('Unsupported personal library');
  return scope(1, owner, encoder.encode(zone[1] ?? ''));
}

export function sharedScope({owner, root, folder}) {
  if (!owner) throw new Error('Missing shared owner');
  return scope(2, owner, concat([folder ? 1 : 0], uuidBytes(root)));
}

export function encodeObjectLink(kind, range, object) {
  if (![1, 2].includes(kind) || range.length !== 16) throw new Error('Invalid object scope');
  const bytes = concat([0x10 | kind], range, uuidBytes(object));
  return prefix + btoa(String.fromCharCode(...bytes)).replaceAll('+', '-').replaceAll('/', '_');
}

export function decodeObjectId(id) {
  if (typeof id !== 'string' || !/^[A-Za-z0-9_-]{44}$/.test(id)) return null;
  const bytes = Uint8Array.from(atob(id.replaceAll('-', '+').replaceAll('_', '/')), c => c.charCodeAt(0));
  const kind = bytes[0] - 0x10;
  if (kind !== 1 && kind !== 2) return null;
  return {kind, scope: hex(bytes.slice(1, 17)), object: uuidString(bytes.slice(17))};
}

export const scopeMatches = (range, target) => hex(range) === target.scope;
