"""Metadata and PDF follow-up through Paperpile's visible, single-reference UI."""
from __future__ import annotations

import re
import time
from urllib.parse import unquote, urlsplit

from playwright.sync_api import Error as BrowserError


def normalized_doi(value: str) -> str:
    value = value.strip()
    if value.lower().startswith(('https://', 'http://')):
        url = urlsplit(value)
        if url.hostname not in ('doi.org', 'dx.doi.org'):
            return ''
        value = unquote(url.path.lstrip('/'))
    value = re.sub(r'^doi:\s*', '', value, flags=re.I)
    return value.lower() if re.fullmatch(r'10\.\d{4,9}/\S+', value, re.I) else ''


def normalized_text(value: str) -> str:
    return ' '.join(value.casefold().split())


def metadata_fields(dialog) -> dict[str, str]:
    # These are rendered edit-dialog rows, not React state or a private API.
    # URLs collapse to just the first entry until their editor is expanded.
    url_label = dialog.get_by_text('URLs', exact=True)
    if url_label.count() == 1:
        url_label.locator('..').locator('[class^="FieldRowValue-"]').click()
        dialog.locator('textarea[name="url"]').wait_for(state='visible')
    return dialog.locator('[class*="FieldRowlabel-"]').evaluate_all("""labels =>
        Object.fromEntries(labels.map(label => [label.innerText.trim(),
            (label.nextElementSibling?.querySelector('textarea, input')?.value ??
             label.nextElementSibling?.innerText ?? '').trim()]))""")


def metadata_changes(dialog) -> dict[str, str]:
    """Reconstruct proposed values from the visible insertion/deletion table."""
    return dialog.get_by_role('row').evaluate_all("""rows => Object.fromEntries(
        rows.map(row => {
            const cells = row.querySelectorAll('td');
            if (cells.length !== 2) return null;
            const value = cells[1].cloneNode(true);
            value.querySelectorAll('del').forEach(e => e.remove());
            return [cells[0].innerText.trim(), value.textContent.trim()];
        }).filter(Boolean))""")


def same_publication(before: dict, changes: dict, title: str, verified_urls: set[str] | None = None) -> bool:
    doi = normalized_doi(before.get('DOI', ''))
    if not doi or normalized_text(title) != normalized_text(before.get('Title', '')):
        return False
    if 'DOI' in changes and normalized_doi(changes['DOI']) != doi:
        return False
    for value in changes.get('URLs', '').split():
        url = urlsplit(value)
        if url.scheme not in ('http', 'https') or not url.hostname:
            return False
        linked_doi = normalized_doi(value)
        if linked_doi and linked_doi != doi:
            return False
        if value not in before.get('URLs', '').split() and linked_doi != doi and value not in (verified_urls or set()):
            return False
    return True


def changes_saved(after: dict, changes: dict) -> bool:
    for key, value in changes.items():
        actual = after.get(key, '')
        # Paperpile adds suggested URLs to the existing list.
        if key == 'URLs' and value.strip():
            if not set(value.split()).issubset(actual.split()):
                return False
        elif normalized_text(actual) != normalized_text(value):
            return False
    return True


class PaperpileEnrichment:
    def __init__(self, ui, *, timeout_ms: int = 30_000, budget_seconds: float = 180):
        self.ui = ui
        self.page = ui.page
        self.timeout_ms = timeout_ms
        self.budget_seconds = budget_seconds

    def targets(self, request: dict) -> list[dict]:
        if request['format'] == 'identifiers':
            return [{'query': item, 'doi': normalized_doi(item)}
                    for item in request['pasteText'].splitlines()]

        # Let Paperpile parse BibTeX/RIS. Show duplicates only in this read-only
        # preview, restore skipping, then cancel; never Import from this dialog.
        self.ui.preview(request['pasteText'])
        dialog = self.page.get_by_role('dialog')
        skip = dialog.get_by_role('checkbox', name=re.compile(r'^Skip .*duplicates?', re.I))
        try:
            if skip.count() and skip.is_checked():
                skip.press('Space')
            grid = dialog.get_by_role('grid')
            grid.wait_for(state='visible', timeout=self.timeout_ms)
            found = {}
            for _ in range(request['referenceCount'] + 1):
                for row in grid.locator('.reference').all():
                    target = {key: row.locator(selector).inner_text().strip()
                              for key, selector in [('title', '.pp-grid-titletext'),
                                                    ('authors', '.reference_authors'),
                                                    ('year', '.reference_year')]}
                    target['query'] = target['title']
                    key = tuple(target.values())
                    found[key] = target
                if len(found) == request['referenceCount']:
                    return list(found.values())
                at_end = grid.evaluate('(e) => e.scrollTop + e.clientHeight >= e.scrollHeight - 1')
                if at_end:
                    break
                grid.evaluate('(e) => { e.scrollTop += e.clientHeight; }')
                self.page.wait_for_timeout(100)  # React virtual-list render.
            raise ValueError('Could not identify every parsed reference for follow-up.')
        finally:
            if skip.count() and not skip.is_checked():
                skip.press('Space')
            self.ui.cancel()

    def select(self, target: dict) -> tuple[object, dict]:
        # A fresh My Library view clears previous selection, filters and dialogs.
        # Searching is incremental; its placeholder disappears after typing.
        from .paperpile import APP_URL
        self.page.goto(APP_URL, wait_until='domcontentloaded')
        if not self.ui.ready():
            raise ValueError('Library is not ready.')
        self.page.locator('input.searchBoxInput-dropdown').fill(target['query'])
        self.page.get_by_role('button', name='1 reference', exact=True).wait_for(
            state='visible', timeout=self.timeout_ms)
        row = self.page.get_by_test_id('reference')
        if row.count() != 1:
            raise ValueError('Reference selection is ambiguous.')
        if target.get('title'):
            for key, selector in [('title', '.pp-grid-titletext'),
                                  ('authors', '.reference_authors'), ('year', '.reference_year')]:
                if normalized_text(row.locator(selector).inner_text()) != normalized_text(target[key]):
                    raise ValueError('The library result differs from the parsed reference.')
        row.get_by_role('checkbox').check()
        self.page.get_by_role('button', name='1 reference · 1 selected', exact=True).wait_for()
        self.page.get_by_role('button', name='Edit', exact=True).click()
        dialog = self.page.get_by_role('dialog')
        dialog.get_by_role('heading', name='Edit metadata', exact=True).wait_for()
        fields = metadata_fields(dialog)
        doi = normalized_doi(fields.get('DOI', ''))
        if target.get('doi'):
            matches = doi == target['doi']
        elif target.get('title'):
            matches = normalized_text(fields.get('Title', '')) == normalized_text(target['title'])
        else:
            matches = target['query'] in fields.get('URLs', '').split()
        dialog.get_by_role('button', name='Cancel', exact=True).click()
        dialog.wait_for(state='hidden')
        if not matches:
            raise ValueError('The selected metadata does not identify the requested publication.')
        return row, fields

    def extension_required(self) -> bool:
        return self.page.get_by_text(
            'This feature requires the browser extension to be installed.', exact=True).is_visible()

    def close_dialog(self) -> None:
        dialog = self.page.get_by_role('dialog')
        for name in ('Cancel', 'Done', 'Close'):
            button = dialog.get_by_role('button', name=name, exact=True)
            if button.count() == 1 and button.is_visible():
                button.click()
                dialog.wait_for(state='hidden')
                return
        raise ValueError('Could not close the metadata result dialog.')

    def verify_urls(self, before: dict, changes: dict) -> set[str]:
        doi = normalized_doi(before.get('DOI', ''))
        verified = set()
        for url in changes.get('URLs', '').split():
            if url in before.get('URLs', '').split() or normalized_doi(url):
                continue
            parsed = urlsplit(url)
            if parsed.scheme not in ('http', 'https') or not parsed.hostname:
                continue
            source = self.page.context.new_page()
            try:
                source.goto(url, wait_until='domcontentloaded', timeout=self.timeout_ms)
                identifiers = source.locator('meta[name], meta[property]').evaluate_all("""metas => metas
                    .filter(e => ['citation_doi', 'dc.identifier', 'dc.identifier.doi', 'prism.doi']
                        .includes((e.getAttribute('name') || e.getAttribute('property')).toLowerCase()))
                    .map(e => e.content)""")
                if doi and any(normalized_doi(value) == doi for value in identifiers):
                    verified.add(url)
            except BrowserError:
                pass  # A blocked or unreadable landing page is not identity proof.
            finally:
                source.close()
        return verified

    def auto_update(self, before: dict) -> dict:
        self.page.get_by_role('button', name='More', exact=True).click()
        self.page.get_by_role('menuitem', name=re.compile(r'^Auto update(?:\s|$)')).click()
        deadline = time.monotonic() + self.timeout_ms / 1000
        while time.monotonic() < deadline:
            if self.extension_required():
                return {'status': 'extension_required'}
            dialog = self.page.get_by_role('dialog')
            if dialog.count() == 1 and dialog.is_visible():
                text = dialog.inner_text()
                if re.search(r'metadata is up.to.date|already up.to.date', text, re.I):
                    self.close_dialog()
                    return {'status': 'up_to_date'}
                if re.search(r'no (?:matching|matches|match found)|could not find', text, re.I):
                    self.close_dialog()
                    return {'status': 'not_found'}
                save = dialog.get_by_role('button', name='Save', exact=True)
                if save.count() == 1 and save.is_enabled():
                    # Save only a same-DOI publication. Unrecognized comparison
                    # layouts and DOI-less records require a human/agent review.
                    proposed = metadata_changes(dialog)
                    title = dialog.locator('.pp-grid-titletext').inner_text()
                    identity = same_publication(before, {k: v for k, v in proposed.items() if k != 'URLs'}, title)
                    verified_urls = self.verify_urls(before, proposed) if identity else set()
                    if not proposed or not same_publication(before, proposed, title, verified_urls):
                        self.close_dialog()
                        return {'status': 'needs_review', 'detail': 'Metadata or publication links could not be verified as the same DOI. No changes saved.'}
                    save.click()
                    dialog.wait_for(state='hidden', timeout=self.timeout_ms)
                    self.page.get_by_role('button', name='Edit', exact=True).click()
                    dialog.get_by_role('heading', name='Edit metadata', exact=True).wait_for()
                    after = metadata_fields(dialog)
                    self.close_dialog()
                    if normalized_doi(after.get('DOI', '')) != normalized_doi(before.get('DOI', '')) or not changes_saved(after, proposed):
                        return {'status': 'unknown', 'detail': 'Save was submitted but metadata readback did not match.'}
                    return {'status': 'updated'}
            self.page.wait_for_timeout(200)
        return {'status': 'pending', 'detail': 'Auto update did not finish before the wait limit.'}

    def find_pdf(self, row) -> dict:
        if row.get_by_role('button', name='PDF', exact=True).is_visible():
            return {'status': 'already_present'}
        status_label = row.locator('.referencePdfMenu_statusWrapper')
        initial_text = status_label.inner_text()
        changed = False
        self.page.get_by_role('button', name='More', exact=True).click()
        self.page.get_by_role('menuitem', name=re.compile(r'^Find PDFs online(?:\s|$)')).click()
        deadline = time.monotonic() + self.timeout_ms / 1000
        while time.monotonic() < deadline:
            if self.extension_required():
                return {'status': 'extension_required'}
            if row.get_by_role('button', name='PDF', exact=True).is_visible():
                return {'status': 'attached'}
            text = status_label.inner_text()
            changed = changed or text != initial_text
            for pattern, status in [('Restricted', 'restricted'), ('Blocked by CAPTCHA', 'captcha'),
                                    ('Not signed into proxy', 'proxy_login_required'),
                                    ('PDF too large', 'too_large'), ('PDF not found', 'not_found')]:
                if changed and pattern.casefold() in text.casefold():
                    return {'status': status}
            self.page.wait_for_timeout(200)
        return {'status': 'pending', 'detail': 'PDF search did not finish before the wait limit.'}

    def run(self, request: dict) -> dict:
        results = []
        deadline = time.monotonic() + self.budget_seconds
        try:
            targets = self.targets(request)
        except (BrowserError, ValueError):
            return {'status': 'incomplete', 'references': [],
                    'detail': 'Registered references could not all be identified for follow-up. No metadata or PDF actions performed.'}
        for target in targets:
            result = {'reference': target['query'], 'metadata': {'status': 'not_run'}, 'pdf': {'status': 'not_run'}}
            results.append(result)
            if time.monotonic() >= deadline:
                result['detail'] = 'Batch wait limit reached. Retry this reference separately.'
                continue
            self.timeout_ms = min(self.timeout_ms, max(1, int((deadline - time.monotonic()) * 1000)))
            stage = 'selection'
            try:
                row, before = self.select(target)
            except (BrowserError, ValueError):
                result['detail'] = 'No unique, matching library reference could be selected. No follow-up actions performed.'
                continue
            try:
                result['metadata'] = self.auto_update(before)
            except (BrowserError, ValueError):
                result['metadata'] = {'status': 'unknown', 'detail': 'Auto update may have started; its result could not be verified. It was not retried.'}
            try:
                # Clear metadata dialogs/timeouts and re-verify identity before
                # PDF search; never let an uncertain selection affect the library.
                stage = 'selection'
                verified_doi = normalized_doi(before.get('DOI', ''))
                pdf_target = {'query': verified_doi, 'doi': verified_doi} if verified_doi else target
                row, _ = self.select(pdf_target)
                stage = 'pdf'
                result['pdf'] = self.find_pdf(row)
            except (BrowserError, ValueError):
                if stage == 'pdf':
                    result[stage] = {'status': 'unknown', 'detail': 'Action may have started; its result could not be verified. It was not retried.'}
                else:
                    result['detail'] = 'No unique, matching library reference could be selected. Follow-up was not continued.'
        completed = all(r['metadata']['status'] in ('updated', 'up_to_date', 'not_found')
                        and r['pdf']['status'] in ('attached', 'already_present', 'restricted', 'not_found', 'too_large')
                        for r in results)
        return {'status': 'completed' if completed else 'incomplete', 'references': results,
                'detail': 'PDF attachment is browser-local; Google Drive synchronization is not verified.'}
