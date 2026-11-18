#!/usr/bin/env python3
"""Serve one signed Fei Notes IPA through an independent Cloudflare tunnel."""
import argparse
import html
import hashlib
import json
import os
import plistlib
import re
import secrets
import signal
import subprocess
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import quote, urlsplit
from ipa_package import verify_ipa


def metadata(ipa):
    info = verify_ipa(ipa)
    if info["CFBundleIdentifier"] not in {"app.quietmd.cloudlab", "app.feinotes.mac"} or info.get("CFBundleDisplayName") != "飞记":
        raise ValueError("Expected a signed Fei Notes app with its recorded product or development identity")
    return info


class InstallServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, ipa):
        self.ipa = ipa
        self.info = metadata(ipa)
        self.digest = hashlib.sha256(ipa.read_bytes()).hexdigest()
        build = self.info["CFBundleVersion"]
        self.prefix = f"/build-{build}/{secrets.token_urlsafe(32)}/"
        self.ipa_name = f"FeiNotes-{self.info['CFBundleShortVersionString']}-{build}-{self.digest[:12]}.ipa"
        self.manifest_name = f"manifest-{build}-{self.digest[:12]}.plist"
        self.origin = None
        super().__init__(("127.0.0.1", 0), InstallHandler)


class InstallHandler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_HEAD(self):
        self.respond(head=True)

    def do_GET(self):
        self.respond(head=False)

    def headers_for(self, status, content_type, length, **headers):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(length))
        self.send_header("Cache-Control", "private, no-store")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("X-Robots-Tag", "noindex, nofollow")
        self.send_header("Content-Security-Policy", "default-src 'none'; style-src 'unsafe-inline'; frame-ancestors 'none'; base-uri 'none'")
        for key, value in headers.items():
            self.send_header(key.replace("_", "-"), value)
        self.end_headers()

    def respond(self, head):
        server = self.server
        prefix = server.prefix
        path = urlsplit(self.path).path
        if not path.startswith(prefix) or not server.origin:
            self.headers_for(404, "text/plain", 0)
            return
        route = path[len(prefix):]
        base = server.origin + prefix
        info = server.info
        if route == server.manifest_name:
            if not head:
                print(f"MANIFEST_REQUESTED build={info['CFBundleVersion']}", flush=True)
            manifest = {"items": [{
                "assets": [{"kind": "software-package", "url": base + server.ipa_name}],
                "metadata": {"bundle-identifier": info["CFBundleIdentifier"],
                             "bundle-version": info["CFBundleVersion"],
                             "kind": "software", "title": "飞记"},
            }]}
            body = plistlib.dumps(manifest)
            content_type = "application/xml; charset=utf-8"
        elif route in ("", "index.html"):
            install = "itms-services://?action=download-manifest&url=" + quote(base + server.manifest_name, safe="")
            version = html.escape(info["CFBundleShortVersionString"])
            build = html.escape(info["CFBundleVersion"])
            size = server.ipa.stat().st_size / 1024 / 1024
            body = f'''<!doctype html><html lang="zh-CN"><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>安装飞记 iOS</title>
<style>body{{font:17px/1.6 -apple-system,BlinkMacSystemFont,sans-serif;color:#222;background:#f6f5f2;margin:0;padding:64px 24px}}main{{max-width:430px;margin:auto}}h1{{font-size:30px;line-height:1.2}}p{{color:#666}}a{{display:block;padding:14px 20px;background:#222;color:white;text-decoration:none;text-align:center;border-radius:12px;margin:32px 0}}small{{color:#777}}</style>
<main><h1>飞记 iPhone / iPad</h1><p>安装本次体验版，检查搜索返回后清空、导航栏、编辑工具栏、iPad 分栏和主题细节。</p>
<a href="{html.escape(install, quote=True)}">安装飞记 {version}（{build}）</a>
<p>请在 Safari 中打开，确认系统安装提示。安装后打开主屏幕上的「飞记」。</p>
<p>更新完成后，请在「设置 → 关于」确认显示 <b>{version}（{build}）</b>。仍显示旧构建号，表示更新尚未完成。</p>
<small>版本 {version} · 构建 {build} · {size:.1f} MB<br>仅适用于已登记的测试 iPhone / iPad。覆盖更新即可，请勿先删除原 App。</small></main></html>'''.encode()
            content_type = "text/html; charset=utf-8"
        elif route == server.ipa_name:
            self.send_ipa(head)
            return
        else:
            self.headers_for(404, "text/plain", 0)
            return
        self.headers_for(200, content_type, len(body))
        if not head:
            self.wfile.write(body)

    def send_ipa(self, head):
        path = self.server.ipa
        total = path.stat().st_size
        start, end, status = 0, total - 1, 200
        byte_range = self.headers.get("Range")
        if byte_range:
            match = re.fullmatch(r"bytes=(\d*)-(\d*)", byte_range)
            if not match or not any(match.groups()):
                self.headers_for(416, "text/plain", 0, Content_Range=f"bytes */{total}")
                return
            left, right = match.groups()
            if left:
                start = int(left)
                end = min(int(right), end) if right else end
            else:
                start = max(0, total - int(right))
            if start > end:
                self.headers_for(416, "text/plain", 0, Content_Range=f"bytes */{total}")
                return
            status = 206
        headers = {"Accept_Ranges": "bytes"}
        if status == 206:
            headers["Content_Range"] = f"bytes {start}-{end}/{total}"
        self.headers_for(status, "application/octet-stream", end - start + 1, **headers)
        if head:
            return
        try:
            with path.open("rb") as stream:
                stream.seek(start)
                remaining = end - start + 1
                while remaining:
                    chunk = stream.read(min(256 * 1024, remaining))
                    if not chunk:
                        break
                    self.wfile.write(chunk)
                    remaining -= len(chunk)
                self.wfile.flush()
                print(f"IPA_SENT build={self.server.info['CFBundleVersion']} bytes={end-start+1-remaining} range={start}-{end}", flush=True)
        except (BrokenPipeError, ConnectionResetError):
            print(f"IPA_INTERRUPTED build={self.server.info['CFBundleVersion']} range={start}-{end}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ipa", type=Path, required=True)
    parser.add_argument("--cloudflared", type=Path, required=True)
    parser.add_argument("--state", type=Path, required=True)
    args = parser.parse_args()
    server = InstallServer(args.ipa.resolve(strict=True))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    command = [str(args.cloudflared.resolve(strict=True)), "tunnel", "--url",
               f"http://127.0.0.1:{server.server_port}", "--protocol", "http2", "--no-autoupdate"]
    tunnel = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)

    def stop(_signal, _frame):
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, stop)
    try:
        for line in tunnel.stdout:
            match = re.search(r"https://[a-z0-9-]+\.trycloudflare\.com", line)
            if match and server.origin is None:
                server.origin = match.group(0)
                url = server.origin + server.prefix
                state = {"url": url, "port": server.server_port, "pid": os.getpid(),
                         "ipa": str(server.ipa), "version": server.info["CFBundleVersion"],
                         "ipa_name": server.ipa_name, "manifest_name": server.manifest_name,
                         "sha256": server.digest}
                with args.state.open("w") as output:
                    args.state.chmod(0o600)
                    json.dump(state, output)
                print("INSTALL_URL=" + url, flush=True)
            if "Registered tunnel connection" in line:
                print("TUNNEL_CONNECTED", flush=True)
            elif "ERR " in line:
                print("Tunnel error, waiting for connection", flush=True)
    except KeyboardInterrupt:
        pass
    finally:
        tunnel.terminate()
        try:
            tunnel.wait(timeout=5)
        except subprocess.TimeoutExpired:
            tunnel.kill()
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
