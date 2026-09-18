"""Exercise the real browser/event protocol against an isolated local UI fixture.

No login or real Paperpile writes. Chrome is optional on engine-only test hosts.
"""
import json
from pathlib import Path
import sys

import pytest
pytest.importorskip("playwright.sync_api", reason="Install the paperpile extra for browser tests")
from filelock import FileLock
from playwright.sync_api import sync_playwright

from bukan_research.paperpile import PaperpileUI, chrome_executable, chrome_profile_path, operate


HTML = """<!doctype html><html><head><meta charset="utf-8"></head><body>
<button id="add">Add</button><button>My Library</button>
<div id="menu" hidden><button role="menuitem" id="paste">Paste… Paste references</button></div>
<div role="dialog" id="dialog" hidden></div>
<script>
const initial = STATE;
const dialog = document.querySelector('#dialog');
let skip = initial.skip;
document.querySelector('#add').onclick = () => document.querySelector('#menu').hidden = false;
document.querySelector('#paste').onclick = () => {
  dialog.hidden = false;
  dialog.innerHTML = '<textarea></textarea><button id="cancel">Cancel</button>';
  document.querySelector('#cancel').onclick = () => dialog.hidden = true;
  dialog.querySelector('textarea').onpaste = e => {
    e.preventDefault();
    window.receivedPaste = e.clipboardData.getData('text/plain');
    render();
  };
};
function render() {
  const dup = initial.duplicates;
  const count = initial.total - (skip ? dup : 0);
  dialog.innerHTML = `<div>${count} ${count === 1 ? 'reference' : 'references'}</div>
    <button>Select all</button><button>Select none</button>
    ${count === 0 && dup ? '<div>All pasted references are duplicates</div>' : ''}
    <div role="combobox">${initial.destination}</div>
    ${dup ? `<label><input type="checkbox" id="skip" ${skip ? 'checked' : ''}>Skip ${dup} duplicates</label>` : ''}
    <button id="cancel">Cancel</button><button id="import">Import</button>`;
  if (dup) document.querySelector('#skip').onchange = e => {skip = e.target.checked; render();};
  document.querySelector('#cancel').onclick = () => dialog.hidden = true;
  document.querySelector('#import').onclick = async () => {
    await fetch('/commit', {method: 'POST'});
    dialog.hidden = true;
  };
}
</script></body></html>"""


@pytest.fixture
def ui_browser():
    chrome = chrome_executable()
    if not chrome:
        pytest.skip("Google Chrome is not installed on this engine-only host")
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=str(chrome), headless=True, chromium_sandbox=True)
        yield browser
        browser.close()


def fixture_page(browser, *, total=2, duplicates=0, skip=True, destination="My Library", commit=True, sync_pending=False):
    state = dict(total=total, duplicates=duplicates, skip=skip, destination=destination)
    calls = []
    context = browser.new_context()
    def route_handler(route):
        if route.request.url.endswith('/commit'):
            calls.append('import')
            if commit:
                state['duplicates'] = state['total']
            route.fulfill(status=200, body='ok')
        else:
            content = HTML.replace('STATE', json.dumps(state))
            if sync_pending:
                content = content.replace('<body>', '<body><div role="progressbar">Syncing library</div>')
            route.fulfill(content_type='text/html', body=content)
    context.route('**/*', route_handler)
    page = context.new_page()
    # Hosted Windows Chrome can need over a second to become actionable.
    # Match the runtime's action timeout; individual timeout tests stay explicit.
    page.set_default_timeout(10_000)
    page.goto('https://paperpile-fixture.test/')
    return context, PaperpileUI(page, timeout_ms=10_000), calls


def test_import_uses_paste_event_and_verifies_after_reload(ui_browser):
    context, ui, calls = fixture_page(ui_browser, duplicates=1, skip=False)
    try:
        result = ui.import_references({'pasteText': '10.1234/a\n10.1234/b', 'referenceCount': 2})
        assert result['status'] == 'present_in_browser'
        assert result['serverSyncVerified'] is False
        assert result['newlyPresentCount'] == 1
        assert result['alreadyPresentCount'] == 1
        assert result['verification']['allDuplicates'] is True
        assert ui.page.evaluate('window.receivedPaste') == '10.1234/a\n10.1234/b'
        assert calls == ['import']  # Verification is a preview, never a second write.
    finally:
        context.close()


def test_visible_toolbar_under_loading_overlay_is_not_ready(ui_browser):
    context, ui, calls = fixture_page(ui_browser)
    try:
        ui.timeout_ms = 500
        ui.page.evaluate("""() => {
            const overlay = document.createElement('div');
            overlay.style.cssText = 'position:fixed;inset:0;z-index:100;background:white';
            overlay.textContent = 'Loading library';
            document.body.append(overlay);
        }""")
        assert ui.ready() is False
        assert calls == []
    finally:
        context.close()


@pytest.mark.parametrize(('options', 'expected'), [
    ({'duplicates': 2}, 'already_present'),
    ({'total': 1}, 'preview_mismatch'),
    ({'destination': 'Shared lab'}, 'ui_unavailable'),
])
def test_unnecessary_partial_or_wrong_destination_imports_do_not_submit(ui_browser, options, expected):
    context, ui, calls = fixture_page(ui_browser, **options)
    try:
        result = ui.import_references({'pasteText': 'two references', 'referenceCount': 2})
        assert result['status'] == expected
        assert result['submissionAttempted'] is False
        assert calls == []
    finally:
        context.close()


def test_unverified_submission_is_unknown_and_not_retried(ui_browser):
    context, ui, calls = fixture_page(ui_browser, commit=False)
    try:
        result = ui.import_references({'pasteText': 'two references', 'referenceCount': 2})
        assert result['status'] == 'unknown'
        assert result['submissionAttempted'] is True
        assert calls == ['import']
    finally:
        context.close()


def test_duplicates_surviving_reload_do_not_prove_background_sync(ui_browser):
    context, ui, calls = fixture_page(ui_browser, sync_pending=True)
    try:
        result = ui.import_references({'pasteText': 'two references', 'referenceCount': 2})
        assert result['status'] == 'present_in_browser'
        assert result['verification']['allDuplicates'] is True
        assert ui.page.get_by_role('progressbar').is_visible()
        assert result['serverSyncVerified'] is False
        assert result['verificationScope'] == 'dedicated-browser'
        assert calls == ['import']
    finally:
        context.close()


def test_explicit_preview_never_submits_new_references(ui_browser):
    context, ui, calls = fixture_page(ui_browser)
    try:
        result = ui.import_references({'pasteText': 'two references', 'referenceCount': 2, 'previewOnly': True})
        assert result['status'] == 'preview_only'
        assert result['preview']['newCount'] == 2
        assert result['submissionAttempted'] is False
        assert calls == []
    finally:
        context.close()


def test_missing_login_and_exclusive_profile(tmp_path, monkeypatch):
    monkeypatch.setattr('bukan_research.paperpile.chrome_executable', lambda: Path('fake-chrome'))
    profile = tmp_path / 'browser' / 'profile'
    assert operate('status', profile, {})['status'] == 'login_required'
    assert not profile.parent.exists()
    profile.mkdir(parents=True)
    with FileLock(str(profile.parent / 'browser.lock')):
        assert operate('import', profile, {})['status'] == 'busy'


def test_missing_chrome_does_not_create_profile(tmp_path, monkeypatch):
    monkeypatch.setattr('bukan_research.paperpile.chrome_executable', lambda: None)
    profile = tmp_path / 'browser' / 'profile'
    assert operate('login', profile, {})['status'] == 'chrome_missing'
    assert not profile.parent.exists()


@pytest.mark.skipif(sys.platform != "win32", reason="Windows path namespaces")
@pytest.mark.parametrize(('source', 'expected'), [
    (r'\\?\C:\研究 O\profile', r'C:\研究 O\profile'),
    (r'\\?\UNC\server\share\profile', r'\\server\share\profile'),
    (r'C:\research\profile', r'C:\research\profile'),
    (r'\\server\share\profile', r'\\server\share\profile'),
])
def test_chrome_paths_preserve_the_same_directory(source, expected):
    assert chrome_profile_path(Path(source)) == expected


@pytest.mark.skipif(sys.platform != "win32", reason="Chromium IndexedDB fails on verbatim Windows paths")
def test_indexeddb_works_across_restarts_for_a_canonicalized_profile(tmp_path):
    chrome = chrome_executable()
    if not chrome:
        pytest.skip("Google Chrome is not installed")
    profile = Path('\\\\?\\' + str(tmp_path / '日本語 profile'))
    with sync_playwright() as p:
        for write in (True, False):
            context = p.chromium.launch_persistent_context(
                chrome_profile_path(profile), executable_path=str(chrome),
                headless=True, chromium_sandbox=True,
            )
            try:
                page = context.pages[0]
                page.route('https://bukan-storage.test/**', lambda route: route.fulfill(
                    content_type='text/html', body='<title>Storage regression test</title>'))
                page.goto('https://bukan-storage.test/')
                result = page.evaluate("""write => new Promise(resolve => {
                    const request = indexedDB.open('bukan-test', 1);
                    const timer = setTimeout(() => resolve({error:'timeout'}), 5000);
                    const done = value => { clearTimeout(timer); resolve(value); };
                    request.onerror = () => done({error: request.error.message});
                    request.onupgradeneeded = () => request.result.createObjectStore('items');
                    request.onsuccess = () => {
                        const db = request.result;
                        const tx = db.transaction('items', write ? 'readwrite' : 'readonly');
                        const store = tx.objectStore('items');
                        const operation = write ? store.put('retained', 'key') : store.get('key');
                        operation.onsuccess = () => { tx.oncomplete = () => { db.close(); done({value:write ? 'written' : operation.result}); }; };
                        tx.onerror = () => { db.close(); done({error:tx.error.message}); };
                    };
                })""", write)
                assert result == {'value': 'written' if write else 'retained'}
            finally:
                context.close()
