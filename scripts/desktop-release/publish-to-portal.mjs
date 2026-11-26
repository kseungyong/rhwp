#!/usr/bin/env node
/**
 * RHWP Studio 데스크톱 릴리스를 사내 포털(portal.kdsys.co.kr)에 게시한다.
 *
 * 산출물 디렉터리에서 설치 파일·업데이트 패키지·minisign 서명을 찾아
 * `rhwp-studio/v<버전>/` 아래로 올리고, 마지막에 latest.json을 올린다 —
 * 클라이언트(앱 자동 업데이트·포털 다운로드 버튼)가 아직 올라가지 않은 파일을
 * 가리키는 순간이 없도록 latest.json이 항상 마지막이다.
 *
 * 사용:
 *   node scripts/desktop-release/publish-to-portal.mjs --version 0.9.2 --dir <산출물 폴더>
 * 환경변수:
 *   PORTAL_UPLOAD_TOKEN  포털 업로드 토큰 (필수)
 *   PORTAL_BASE_URL      기본 https://portal.kdsys.co.kr
 *
 * 산출물 폴더는 재귀로 뒤지므로 Tauri bundle 디렉터리를 그대로 줘도 되고,
 * CI가 아티팩트를 모아둔 평면 폴더를 줘도 된다.
 */
import { readdir, readFile, stat } from 'node:fs/promises';
import { join, basename } from 'node:path';
import process from 'node:process';

function arg(name) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

const version = arg('version');
const dir = arg('dir');
const token = process.env.PORTAL_UPLOAD_TOKEN;
const baseUrl = (process.env.PORTAL_BASE_URL ?? 'https://portal.kdsys.co.kr').replace(/\/$/, '');

if (!version || !dir) {
  console.error('사용법: publish-to-portal.mjs --version <버전> --dir <산출물 폴더>');
  process.exit(2);
}
if (!token) {
  console.error('PORTAL_UPLOAD_TOKEN 환경변수가 필요합니다.');
  process.exit(2);
}

/** 디렉터리를 재귀로 뒤져 파일 전체 경로 목록을 준다. */
async function walk(root) {
  const out = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const full = join(root, entry.name);
    if (entry.isDirectory()) out.push(...(await walk(full)));
    else if (entry.isFile()) out.push(full);
  }
  return out;
}

const files = await walk(dir);
const find = (suffix) => files.find((f) => f.endsWith(suffix));

// Windows: NSIS 설치 파일이 업데이트 패키지를 겸한다.
const winSetup = find('-setup.exe');
const winSig = find('-setup.exe.sig');
// macOS: 설치는 .dmg, 업데이트 패키지는 .app.tar.gz.
const macDmg = find('.dmg');
const macUpdate = find('.app.tar.gz');
const macSig = find('.app.tar.gz.sig');

if (!winSetup !== !winSig) throw new Error('Windows 산출물 불완전: setup.exe와 .sig가 쌍이어야 합니다.');
if (!macUpdate !== !macSig) throw new Error('macOS 산출물 불완전: .app.tar.gz와 .sig가 쌍이어야 합니다.');
if (!winSetup && !macUpdate) throw new Error(`산출물을 찾지 못했습니다: ${dir}`);

const storePrefix = `rhwp-studio/v${version}`;
const publicUrl = (name) =>
  `${baseUrl}/api/downloads/${storePrefix}/${encodeURIComponent(name)}`;

async function upload(relPath, body) {
  const url = `${baseUrl}/api/downloads/upload?path=${encodeURIComponent(relPath)}`;
  const res = await fetch(url, {
    method: 'PUT',
    headers: { 'x-upload-token': token },
    body,
  });
  if (!res.ok) throw new Error(`업로드 실패 ${res.status} ${relPath}: ${await res.text()}`);
  console.log(`올림: ${relPath} (${body.length.toLocaleString()} bytes)`);
}

const latest = {
  version,
  pub_date: new Date().toISOString(),
  platforms: {},
  installers: {},
};

const uploads = [];
if (winSetup) {
  uploads.push([`${storePrefix}/${basename(winSetup)}`, winSetup]);
  latest.platforms['windows-x86_64'] = {
    signature: (await readFile(winSig, 'utf-8')).trim(),
    url: publicUrl(basename(winSetup)),
  };
  latest.installers.windows = publicUrl(basename(winSetup));
}
if (macUpdate) {
  uploads.push([`${storePrefix}/${basename(macUpdate)}`, macUpdate]);
  // universal(arm64+x86_64) 단일 패키지 — Tauri 업데이터는 darwin-<자기 아키텍처>
  // 키를 먼저 찾으므로, 같은 패키지를 세 키 모두에 등록해 어느 맥에서도 찾게 한다.
  const macPlatform = {
    signature: (await readFile(macSig, 'utf-8')).trim(),
    url: publicUrl(basename(macUpdate)),
  };
  latest.platforms['darwin-aarch64'] = macPlatform;
  latest.platforms['darwin-x86_64'] = macPlatform;
  latest.platforms['darwin-universal'] = macPlatform;
}
if (macDmg) {
  uploads.push([`${storePrefix}/${basename(macDmg)}`, macDmg]);
  latest.installers.macos = publicUrl(basename(macDmg));
}

for (const [relPath, localPath] of uploads) {
  const size = (await stat(localPath)).size;
  console.log(`읽는 중: ${localPath} (${size.toLocaleString()} bytes)`);
  await upload(relPath, await readFile(localPath));
}

// latest.json은 반드시 마지막에.
await upload('rhwp-studio/latest.json', Buffer.from(JSON.stringify(latest, null, 2)));
console.log(`게시 완료: v${version}`);
console.log(`다운로드 확인: ${baseUrl}/api/downloads/rhwp-studio/latest-setup/windows`);
