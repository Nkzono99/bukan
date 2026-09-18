"""Paperpile follow-up writes run only against synthetic local browser pages."""
import json

import pytest
pytest.importorskip('playwright.sync_api')
from playwright.sync_api import sync_playwright

from bukan_research.paperpile import PaperpileUI, chrome_executable
from bukan_research.paperpile_enrichment import PaperpileEnrichment


HTML = '''<!doctype html><html><head><meta charset="utf-8"></head><body>
<button>Add</button><button>My Library</button>
<input class="searchBoxInput-dropdown" placeholder="Search library">
<button id="count">1 reference</button><button id="edit">Edit</button><button id="more">More</button>
<div role="grid"><div data-testid="reference">
<span class="pp-grid-titletext">Dust study</span><span class="reference_authors">Author A</span>
<span class="reference_year">2025</span><input type="checkbox" id="select"><span id="pdf" class="referencePdfMenu_statusWrapper"></span>
</div></div><div role="menu" hidden id="menu">
<button role="menuitem" id="update">Auto update U</button>
<button role="menuitem" id="find">Find PDFs online D</button></div>
<div role="dialog" hidden id="dialog"></div><div id="notice"></div>
<script>
const state = STATE;
document.querySelector('.reference_year').textContent = state.metadata.Year || '2025';
document.querySelector('.pp-grid-titletext').textContent = state.metadata.Title;
const dialog = document.querySelector('#dialog');
const menu = document.querySelector('#menu');
const report = action => fetch('/action', {method:'POST', body: action});
const fields = values => Object.entries(values).map(([key,value]) =>
  `<div><div class="FieldRowlabel-fixture">${key}</div><div class="FieldRowValue-fixture">${key === 'URLs' ? value.split('\\n')[0] : value}</div></div>`).join('');
const changes = values => '<span class="pp-grid-titletext">Dust study</span><table>' + Object.entries(values).map(([key,value]) =>
  `<tr><td>${key}</td><td><del>${state.metadata[key] || ''}</del><ins>${value}</ins></td></tr>`).join('') + '</table>';
function showPdf() { document.querySelector('#pdf').innerHTML = state.pdf ? '<button>PDF</button>' : '<button>Add PDF</button>'; }
showPdf();
document.querySelector('#select').onchange = () => document.querySelector('#count').textContent = '1 reference · 1 selected';
document.querySelector('#edit').onclick = () => {
  dialog.innerHTML = '<h2>Edit metadata</h2>' + fields(state.metadata) + '<button id="cancel">Cancel</button>';
  dialog.hidden = false;
  const urlLabel = [...dialog.querySelectorAll('.FieldRowlabel-fixture')].find(e => e.textContent === 'URLs');
  if (urlLabel) urlLabel.nextElementSibling.onclick = () => {
    const textarea = document.createElement('textarea'); textarea.name = 'url'; textarea.value = state.metadata.URLs;
    urlLabel.nextElementSibling.replaceChildren(textarea);
  };
  document.querySelector('#cancel').onclick = () => dialog.hidden = true;
};
document.querySelector('#more').onclick = () => menu.hidden = false;
document.querySelector('#update').onclick = async () => {
  menu.hidden = true;
  await report('update');
  if (state.extensionMissing) {
    document.querySelector('#notice').textContent = 'This feature requires the browser extension to be installed.';
    return;
  }
  dialog.hidden = false;
  if (state.unchanged) {
    dialog.innerHTML = 'Metadata is up-to-date<button id="cancel">Done</button>';
  } else {
    dialog.innerHTML = changes(state.proposed) + '<button id="cancel">Cancel</button><button id="save">Save</button>';
    document.querySelector('#save').onclick = async () => { const response = await report('save'); state.metadata = await response.json(); dialog.hidden = true; };
  }
  document.querySelector('#cancel').onclick = () => dialog.hidden = true;
};
document.querySelector('#find').onclick = async () => {
  menu.hidden = true;
  await report('pdf');
  if (state.extensionMissing) {
    document.querySelector('#notice').textContent = 'This feature requires the browser extension to be installed.';
  } else if (state.pdfFailure) {
    document.querySelector('#pdf').textContent = state.pdfFailure;
  } else {
    state.pdf = true;
    showPdf();
  }
};
</script></body></html>'''


@pytest.fixture(scope='module')
def browser():
    chrome = chrome_executable()
    if not chrome:
        pytest.skip('Google Chrome is not installed')
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=str(chrome), headless=True, chromium_sandbox=True)
        yield browser
        browser.close()


@pytest.fixture
def fixture(browser):
    contexts = []
    def create(**options):
        metadata = {'Title': 'Dust study', 'DOI': '10.1234/dust', 'Year': '2025'}
        state = dict(metadata=metadata, proposed={**metadata, 'Year': '2026'}, pdf=False,
                     extensionMissing=False, unchanged=False, pdfFailure='')
        state.update(options)
        calls = []
        context = browser.new_context()
        contexts.append(context)
        def route(route):
            if route.request.url == 'https://journal.test/articles/dust-study':
                route.fulfill(content_type='text/html', body='<meta name="citation_doi" content="10.1234/dust">')
            elif route.request.url.endswith('/action'):
                action = route.request.post_data
                calls.append(action)
                if action == 'save':
                    old_urls = state['metadata'].get('URLs', '')
                    state['metadata'] = {**state['metadata'], **state['proposed']}
                    if old_urls and state['proposed'].get('URLs'):
                        state['metadata']['URLs'] = old_urls + '\n' + state['proposed']['URLs']
                elif action == 'pdf' and not state['pdfFailure'] and not state['extensionMissing']:
                    state['pdf'] = True
                route.fulfill(body=json.dumps(state['metadata']))
            else:
                route.fulfill(content_type='text/html', body=HTML.replace('STATE', json.dumps(state)))
        context.route('**/*', route)
        page = context.new_page()
        page.set_default_timeout(10_000)
        ui = PaperpileUI(page)
        return ui, calls, state
    yield create
    for context in contexts:
        context.close()


REQUEST = {'format': 'identifiers', 'pasteText': '10.1234/dust', 'referenceCount': 1}


def test_registration_runs_metadata_then_pdf_and_reads_back(fixture, monkeypatch):
    ui, calls, state = fixture()
    monkeypatch.setattr(ui, 'import_references', lambda _: {'status': 'present_in_browser'})
    result = ui.register_and_enrich(REQUEST)
    assert result['status'] == 'present_in_browser'
    assert result['postprocessing']['status'] == 'completed'
    record = result['postprocessing']['references'][0]
    assert record['metadata']['status'] == 'updated'
    assert record['pdf']['status'] == 'attached'
    assert state['metadata']['Year'] == '2026'
    assert calls == ['update', 'save', 'pdf']


def test_existing_reference_keeps_its_pdf(fixture, monkeypatch):
    ui, calls, _ = fixture(pdf=True, unchanged=True)
    monkeypatch.setattr(ui, 'import_references', lambda _: {'status': 'already_present'})
    result = ui.register_and_enrich(REQUEST)
    assert result['postprocessing']['references'][0]['pdf']['status'] == 'already_present'
    assert calls == ['update']


@pytest.mark.parametrize('options', [{'previewOnly': True}, {'postprocess': False}])
def test_preview_and_opt_out_do_not_run_followup(fixture, monkeypatch, options):
    ui, calls, _ = fixture()
    monkeypatch.setattr(ui, 'import_references', lambda _: {'status': 'already_present'})
    result = ui.register_and_enrich({**REQUEST, **options})
    assert result['postprocessing']['status'] == 'not_requested'
    assert calls == []


@pytest.mark.parametrize('status', ['unknown', 'preview_mismatch', 'ui_unavailable'])
def test_unverified_import_does_not_run_followup(fixture, monkeypatch, status):
    ui, calls, _ = fixture()
    monkeypatch.setattr(ui, 'import_references', lambda _: {'status': status})
    assert ui.register_and_enrich(REQUEST)['postprocessing']['status'] == 'not_run'
    assert calls == []


def test_wrong_library_identity_is_never_updated(fixture):
    ui, calls, _ = fixture(metadata={'Title': 'Other paper', 'DOI': '10.1234/other'})
    result = PaperpileEnrichment(ui).run(REQUEST)
    assert result['status'] == 'incomplete'
    assert result['references'][0]['metadata']['status'] == 'not_run'
    assert calls == []


def test_different_doi_suggestion_is_not_saved_but_pdf_is_searched(fixture):
    ui, calls, state = fixture(proposed={'Title': 'Another publication', 'DOI': '10.1234/other'})
    result = PaperpileEnrichment(ui).run(REQUEST)
    assert result['references'][0]['metadata']['status'] == 'needs_review'
    assert result['references'][0]['pdf']['status'] == 'attached'
    assert state['metadata']['DOI'] == '10.1234/dust'
    assert calls == ['update', 'pdf']


def test_missing_extension_is_not_reported_as_complete(fixture):
    ui, calls, _ = fixture(extensionMissing=True)
    result = PaperpileEnrichment(ui).run(REQUEST)
    assert result['status'] == 'incomplete'
    assert result['references'][0]['metadata']['status'] == 'extension_required'
    assert result['references'][0]['pdf']['status'] == 'extension_required'
    assert calls == ['update', 'pdf']


@pytest.mark.parametrize(('message', 'status'), [('Restricted', 'restricted'),
    ('Blocked by CAPTCHA', 'captcha'), ('PDF not found', 'not_found')])
def test_pdf_search_reports_access_result(fixture, message, status):
    ui, calls, _ = fixture(unchanged=True, pdfFailure=message)
    result = PaperpileEnrichment(ui).run(REQUEST)
    assert result['references'][0]['pdf']['status'] == status
    assert calls == ['update', 'pdf']


def test_update_table_can_change_fields_without_repeating_unchanged_doi(fixture):
    ui, calls, _ = fixture(proposed={'Year': '2026'})
    result = PaperpileEnrichment(ui).run(REQUEST)
    assert result['references'][0]['metadata']['status'] == 'updated'
    assert calls == ['update', 'save', 'pdf']


@pytest.mark.parametrize('url', ['https://example.org/authors/a', 'https://doi.org/10.1234/other',
                               'https://researchmap.jp/some-author', 'https://journal.test/articles/unrelated'])
def test_unrelated_link_suggestion_is_not_saved(fixture, url):
    ui, calls, _ = fixture(proposed={'URLs': url})
    result = PaperpileEnrichment(ui).run(REQUEST)
    assert result['references'][0]['metadata']['status'] == 'needs_review'
    assert calls == ['update', 'pdf']


def test_metadata_ui_failure_still_allows_independent_pdf_search(fixture, monkeypatch):
    ui, calls, _ = fixture()
    helper = PaperpileEnrichment(ui)
    def broken(_):
        raise ValueError('Synthetic failed metadata lookup')
    monkeypatch.setattr(helper, 'auto_update', broken)
    result = helper.run(REQUEST)
    assert result['references'][0]['metadata']['status'] == 'unknown'
    assert result['references'][0]['pdf']['status'] == 'attached'
    assert calls == ['pdf']


def test_time_budget_reports_unprocessed_references(fixture):
    ui, calls, _ = fixture()
    result = PaperpileEnrichment(ui, budget_seconds=0).run(REQUEST)
    assert result['status'] == 'incomplete'
    assert result['references'][0]['metadata']['status'] == 'not_run'
    assert calls == []


@pytest.mark.parametrize('format', ['bibtex', 'ris'])
@pytest.mark.parametrize('unchanged', [True, False])
def test_bibliography_targets_come_from_preview_and_duplicates_are_restored(fixture, monkeypatch, format, unchanged):
    ui, calls, _ = fixture(unchanged=unchanged)
    def preview(_):
        ui.page.goto('https://paperpile-fixture.test/')
        ui.page.evaluate('''() => {
            const dialog = document.querySelector('#dialog');
            dialog.hidden = false;
            dialog.innerHTML = `<label><input type="checkbox" checked>Skip 1 duplicates</label>
                <div role="grid"><div class="reference"><span class="pp-grid-titletext">Dust study</span>
                <span class="reference_authors">Author A</span><span class="reference_year">2025</span></div></div>
                <button id="cancel">Cancel</button>`;
            document.querySelector('#cancel').onclick = () => {
                if (!dialog.querySelector('input').checked) throw Error('Duplicate skipping not restored');
                dialog.hidden = true;
            };
        }''')
    monkeypatch.setattr(ui, 'preview', preview)
    result = PaperpileEnrichment(ui).run({**REQUEST, 'format': format})
    assert result['status'] == 'completed'
    assert result['references'][0]['reference'] == 'Dust study'
    assert calls == (['update', 'pdf'] if unchanged else ['update', 'save', 'pdf'])


def test_unresolved_bibliography_does_not_guess_from_the_library(fixture, monkeypatch):
    ui, calls, _ = fixture()
    monkeypatch.setattr(ui, 'preview', lambda _: (_ for _ in ()).throw(ValueError('Unrecognized preview')))
    result = PaperpileEnrichment(ui).run({**REQUEST, 'format': 'bibtex'})
    assert result['status'] == 'incomplete'
    assert result['references'] == []
    assert calls == []


def test_appended_urls_are_read_from_expanded_editor(fixture):
    original = 'https://doi.org/10.1234/dust'
    added = 'https://journal.test/articles/dust-study'
    ui, calls, state = fixture(metadata={'Title': 'Dust study', 'DOI': '10.1234/dust', 'URLs': original},
                               proposed={'URLs': added})
    result = PaperpileEnrichment(ui).run(REQUEST)
    assert result['references'][0]['metadata']['status'] == 'updated'
    assert state['metadata']['URLs'].splitlines() == [original, added]
    assert calls == ['update', 'save', 'pdf']


def test_pdf_error_words_in_title_are_not_a_download_status(fixture):
    ui, calls, _ = fixture(metadata={'Title': 'Restricted diffusion study', 'DOI': '10.1234/dust'},
                           pdfFailure='Searching for PDF')
    helper = PaperpileEnrichment(ui)
    row, _ = helper.select({'query': '10.1234/dust', 'doi': '10.1234/dust'})
    helper.timeout_ms = 500
    assert helper.find_pdf(row)['status'] == 'pending'
    assert calls == ['pdf']
